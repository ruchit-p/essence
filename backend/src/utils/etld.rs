//! eTLD+1 (Effective Top-Level Domain + 1) extraction using Public Suffix List
//!
//! This module provides proper domain grouping by extracting the registrable domain
//! from URLs. This is critical for:
//! - Domain rate limiting (group all subdomains together)
//! - Robots.txt caching (share cache across subdomains)
//! - Analytics and statistics
//!
//! Examples:
//! - `www.example.com` → `example.com`
//! - `api.subdomain.example.co.uk` → `example.co.uk`
//! - `github.io` → `github.io` (public suffix itself)

use crate::error::{Result, ScrapeError};
use once_cell::sync::Lazy;
use publicsuffix::{List, Psl};
use std::sync::Arc;
use tracing::debug;
use url::Url;

/// Global Public Suffix List (loaded once, thread-safe)
static PSL: Lazy<Arc<List>> = Lazy::new(|| {
    // Use the bundled PSL data from the publicsuffix crate
    let list = List::new();
    Arc::new(list)
});

/// Extract eTLD+1 (registrable domain) from a URL
///
/// This uses the Public Suffix List to correctly identify the registrable
/// domain portion of a URL, accounting for complex TLDs like .co.uk
///
/// # Examples
///
/// ```
/// use essence::utils::etld::extract_etld_plus_one;
///
/// assert_eq!(
///     extract_etld_plus_one("https://www.example.com/path").unwrap(),
///     "example.com"
/// );
///
/// assert_eq!(
///     extract_etld_plus_one("https://api.subdomain.example.co.uk").unwrap(),
///     "example.co.uk"
/// );
/// ```
pub fn extract_etld_plus_one(url_str: &str) -> Result<String> {
    // Parse URL
    let url = Url::parse(url_str)
        .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid URL: {}", e)))?;

    // Get host
    let host = url
        .host_str()
        .ok_or_else(|| ScrapeError::InvalidUrl("No host in URL".to_string()))?;

    // Use Psl::domain() to get the registrable domain
    match PSL.domain(host.as_bytes()) {
        Some(domain) => {
            let etld_plus_one = std::str::from_utf8(domain.as_bytes())
                .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid domain encoding: {}", e)))?
                .to_string();
            debug!("Extracted eTLD+1: {} from {}", etld_plus_one, url_str);
            Ok(etld_plus_one)
        }
        None => {
            // If PSL can't determine the domain, use the host as-is
            debug!("PSL could not determine domain for {}, using host: {}", url_str, host);
            Ok(host.to_string())
        }
    }
}

/// Extract eTLD+1 from a URL, with fallback to hostname if PSL parsing fails
///
/// This is a more forgiving version that won't fail on edge cases.
/// Use this for non-critical paths where having *some* domain grouping
/// is better than failing.
pub fn extract_etld_plus_one_or_host(url_str: &str) -> Result<String> {
    match extract_etld_plus_one(url_str) {
        Ok(etld) => Ok(etld),
        Err(_) => {
            // Fallback: just use the hostname
            let url = Url::parse(url_str)
                .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid URL: {}", e)))?;

            let host = url
                .host_str()
                .ok_or_else(|| ScrapeError::InvalidUrl("No host in URL".to_string()))?;

            debug!("eTLD+1 extraction failed, using hostname: {}", host);
            Ok(host.to_string())
        }
    }
}

/// Check if a domain is a public suffix (like .com, .co.uk, github.io)
pub fn is_public_suffix(domain: &str) -> bool {
    // A domain is a public suffix if PSL.domain() returns None for it
    // (meaning there's no registrable domain above the suffix)
    PSL.domain(domain.as_bytes()).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_etld_basic() {
        assert_eq!(
            extract_etld_plus_one("https://www.example.com/path").unwrap(),
            "example.com"
        );

        assert_eq!(
            extract_etld_plus_one("https://subdomain.example.com").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn test_extract_etld_complex_tld() {
        assert_eq!(
            extract_etld_plus_one("https://www.example.co.uk/path").unwrap(),
            "example.co.uk"
        );

        assert_eq!(
            extract_etld_plus_one("https://api.example.co.jp").unwrap(),
            "example.co.jp"
        );
    }

    #[test]
    fn test_extract_etld_public_suffix() {
        // github.io is a public suffix in the PSL
        let result = extract_etld_plus_one("https://username.github.io").unwrap();
        // Should return username.github.io (the registrable part)
        assert_eq!(result, "username.github.io");
    }

    #[test]
    fn test_extract_etld_ip_address() {
        // IP addresses should work
        let result = extract_etld_plus_one("http://192.168.1.1/path");
        // IP addresses aren't in PSL, so this returns the IP itself
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_etld_localhost() {
        let result = extract_etld_plus_one("http://localhost:8080");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "localhost");
    }

    #[test]
    fn test_is_public_suffix() {
        assert!(!is_public_suffix("example.com"));
        assert!(!is_public_suffix("google.co.uk"));
        // Note: actual public suffix detection depends on the PSL data
    }

    #[test]
    fn test_extract_etld_or_host_fallback() {
        // Should work for normal URLs
        assert_eq!(
            extract_etld_plus_one_or_host("https://www.example.com").unwrap(),
            "example.com"
        );

        // Should fall back to hostname for edge cases
        // This test mainly verifies the function doesn't panic
        let result = extract_etld_plus_one_or_host("https://weird-domain-123.local");
        assert!(result.is_ok());
    }
}
