pub mod dns_cache;
pub mod etld;
pub mod retry;
pub mod robots;
pub mod robots_enhanced;
pub mod ssrf_protection;
pub mod url_rewrites;
pub mod user_agents;

use url::Url;

/// Normalize URL to prevent duplicates from trailing slashes, fragments, etc.
///
/// This function:
/// - Removes ALL trailing slashes (including root path)
/// - Removes fragments (#anchors)
/// - Lowercases the scheme and host
/// - Preserves query parameters
///
/// # Examples
///
/// ```
/// use essence::utils::normalize_url_string;
///
/// assert_eq!(
///     normalize_url_string("https://example.com/").unwrap(),
///     "https://example.com"
/// );
/// assert_eq!(
///     normalize_url_string("https://example.com/page/").unwrap(),
///     "https://example.com/page"
/// );
/// assert_eq!(
///     normalize_url_string("https://example.com/page#section").unwrap(),
///     "https://example.com/page"
/// );
/// ```
pub fn normalize_url_string(url_str: &str) -> Result<String, String> {
    let mut url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {}", e))?;

    // Remove fragment (#anchors)
    url.set_fragment(None);

    // Remove trailing slash from path (for non-root paths)
    let path = url.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        url.set_path(&path[..path.len() - 1]);
    }

    // Serialize and remove trailing slash even for root path
    // This ensures "https://example.com" and "https://example.com/" are the same
    let mut normalized = url.to_string();
    if normalized.ends_with('/') && !normalized.ends_with("://") {
        normalized.pop();
    }

    Ok(normalized)
}

/// Parse and normalize URL (legacy function for backward compatibility)
pub fn normalize_url(url_str: &str) -> Result<Url, String> {
    let normalized = normalize_url_string(url_str)?;
    Url::parse(&normalized).map_err(|e| format!("Invalid URL: {}", e))
}

/// Extract domain from URL
pub fn extract_domain(url: &Url) -> Option<String> {
    url.host_str().map(|s| s.to_string())
}

/// Check if URL is valid for scraping
pub fn is_valid_scrape_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url_trailing_slash() {
        // Root path should have trailing slash removed for consistency
        assert_eq!(
            normalize_url_string("https://example.com/").unwrap(),
            "https://example.com"
        );

        assert_eq!(
            normalize_url_string("https://example.com").unwrap(),
            "https://example.com"
        );

        // Non-root paths should have trailing slash removed
        assert_eq!(
            normalize_url_string("https://example.com/page/").unwrap(),
            "https://example.com/page"
        );

        assert_eq!(
            normalize_url_string("https://example.com/blog/post/").unwrap(),
            "https://example.com/blog/post"
        );
    }

    #[test]
    fn test_normalize_url_fragment() {
        assert_eq!(
            normalize_url_string("https://example.com#section").unwrap(),
            "https://example.com"
        );

        assert_eq!(
            normalize_url_string("https://example.com/page#section").unwrap(),
            "https://example.com/page"
        );

        assert_eq!(
            normalize_url_string("https://example.com/page/#section").unwrap(),
            "https://example.com/page"
        );
    }

    #[test]
    fn test_normalize_url_query_params() {
        // Query parameters should be preserved
        assert_eq!(
            normalize_url_string("https://example.com/page?key=value").unwrap(),
            "https://example.com/page?key=value"
        );

        assert_eq!(
            normalize_url_string("https://example.com/page/?key=value").unwrap(),
            "https://example.com/page?key=value"
        );
    }

    #[test]
    fn test_normalize_url_scheme_and_host() {
        // Scheme and host should be lowercased
        assert_eq!(
            normalize_url_string("HTTPS://EXAMPLE.COM/PAGE").unwrap(),
            "https://example.com/PAGE"
        );
    }

    #[test]
    fn test_normalize_url_deduplication_case() {
        // This is the critical test case from quotes.toscrape.com
        let url1 = normalize_url_string("https://quotes.toscrape.com").unwrap();
        let url2 = normalize_url_string("https://quotes.toscrape.com/").unwrap();

        assert_eq!(
            url1, url2,
            "URLs with and without trailing slash should normalize to the same value"
        );
    }

    #[test]
    fn test_normalize_url_invalid() {
        assert!(normalize_url_string("not a url").is_err());
        assert!(normalize_url_string("").is_err());
    }
}
