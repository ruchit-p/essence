use crate::error::{Result, ScrapeError};
use std::net::{IpAddr, Ipv6Addr};
use url::Url;

/// Check if an IP address is private/internal
pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_private()           // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
            || ipv4.is_loopback()       // 127.0.0.0/8
            || ipv4.is_link_local()     // 169.254.0.0/16
            || ipv4.is_documentation()  // 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
            || ipv4.is_broadcast()      // 255.255.255.255
            || ipv4.octets()[0] == 0 // 0.0.0.0/8 (reserved)
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()          // ::1
            || ipv6.is_unspecified()    // ::
            || is_ipv6_unique_local(ipv6)  // fc00::/7
            || is_ipv6_link_local(ipv6) // fe80::/10
        }
    }
}

fn is_ipv6_unique_local(ipv6: &Ipv6Addr) -> bool {
    (ipv6.octets()[0] & 0xfe) == 0xfc
}

fn is_ipv6_link_local(ipv6: &Ipv6Addr) -> bool {
    (ipv6.octets()[0] & 0xfe) == 0xfe && (ipv6.octets()[1] & 0xc0) == 0x80
}

/// Common internal hostnames that should be blocked
const BLOCKED_HOSTNAMES: &[&str] = &[
    "localhost",
    "metadata.google.internal", // GCP metadata
    "169.254.169.254",          // AWS metadata IP
    "metadata",
    "instance-data", // AWS instance data
    "consul",        // Consul service discovery
    "vault",         // HashiCorp Vault
];

/// Check if hostname is in blocklist
pub fn is_blocked_hostname(hostname: &str) -> bool {
    let lower = hostname.to_lowercase();
    BLOCKED_HOSTNAMES
        .iter()
        .any(|&blocked| lower == blocked || lower.ends_with(&format!(".{}", blocked)))
}

/// Validate URL and check for SSRF vulnerabilities
pub async fn validate_url_safe(url: &str) -> Result<()> {
    // Parse URL
    let parsed =
        Url::parse(url).map_err(|e| ScrapeError::InvalidUrl(format!("Invalid URL: {}", e)))?;

    // Only allow http/https
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(ScrapeError::InvalidUrl(format!(
                "Unsupported scheme: {}. Only http/https allowed.",
                scheme
            )));
        }
    }

    // Check for IP address in URL
    if let Some(host) = parsed.host_str() {
        // Check hostname blocklist first
        if is_blocked_hostname(host) {
            return Err(ScrapeError::SsrfAttempt(format!(
                "Access to blocked hostname '{}' is not allowed",
                host
            )));
        }

        // Try to parse as IP address
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_private_ip(&ip) {
                return Err(ScrapeError::SsrfAttempt(format!(
                    "Access to private IP {} is not allowed",
                    ip
                )));
            }
        }

        // DNS lookup and check resolved IPs
        // This prevents DNS rebinding attacks where a domain resolves to a private IP
        match tokio::net::lookup_host(format!("{}:80", host)).await {
            Ok(addrs) => {
                for addr in addrs {
                    if is_private_ip(&addr.ip()) {
                        return Err(ScrapeError::SsrfAttempt(format!(
                            "Domain {} resolves to private IP {}",
                            host,
                            addr.ip()
                        )));
                    }
                }
            }
            Err(e) => {
                // If DNS lookup fails, proceed (will fail later with proper error)
                tracing::warn!("DNS lookup failed for {}: {}", host, e);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_private_ipv4() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }

    #[test]
    fn test_public_ipv4() {
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
    }

    #[tokio::test]
    async fn test_block_private_ip_url() {
        assert!(validate_url_safe("http://127.0.0.1/test").await.is_err());
        assert!(validate_url_safe("http://192.168.1.1/test").await.is_err());
        assert!(validate_url_safe("http://10.0.0.1/test").await.is_err());
    }

    #[tokio::test]
    async fn test_allow_public_url() {
        assert!(validate_url_safe("https://example.com").await.is_ok());
    }

    #[tokio::test]
    async fn test_block_invalid_scheme() {
        assert!(validate_url_safe("file:///etc/passwd").await.is_err());
        assert!(validate_url_safe("ftp://example.com").await.is_err());
    }

    #[test]
    fn test_blocked_hostnames() {
        assert!(is_blocked_hostname("localhost"));
        assert!(is_blocked_hostname("LOCALHOST"));
        assert!(is_blocked_hostname("metadata.google.internal"));
        assert!(is_blocked_hostname("metadata"));
        assert!(is_blocked_hostname("169.254.169.254"));
        assert!(!is_blocked_hostname("google.com"));
        assert!(!is_blocked_hostname("example.com"));
    }

    #[tokio::test]
    async fn test_block_metadata_endpoints() {
        assert!(
            validate_url_safe("http://metadata.google.internal/computeMetadata/v1/")
                .await
                .is_err()
        );
        assert!(
            validate_url_safe("http://169.254.169.254/latest/meta-data/")
                .await
                .is_err()
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_ssrf_protection_comprehensive() {
        // Block localhost
        assert!(validate_url_safe("http://localhost/secret").await.is_err());
        assert!(validate_url_safe("http://127.0.0.1/secret").await.is_err());
        assert!(validate_url_safe("http://127.0.0.1:8080/admin")
            .await
            .is_err());

        // Block private networks
        assert!(validate_url_safe("http://192.168.0.1/router")
            .await
            .is_err());
        assert!(validate_url_safe("http://192.168.1.1/admin").await.is_err());
        assert!(validate_url_safe("http://10.0.0.1/internal").await.is_err());
        assert!(validate_url_safe("http://172.16.0.1/vpn").await.is_err());

        // Block link-local
        assert!(validate_url_safe("http://169.254.169.254/metadata")
            .await
            .is_err());

        // Block invalid schemes
        assert!(validate_url_safe("file:///etc/passwd").await.is_err());
        assert!(validate_url_safe("ftp://internal.server/data")
            .await
            .is_err());
        assert!(validate_url_safe("ssh://server.local/").await.is_err());

        // Allow public URLs
        assert!(validate_url_safe("https://example.com").await.is_ok());
        assert!(validate_url_safe("https://www.google.com").await.is_ok());
        assert!(validate_url_safe("http://github.com").await.is_ok());
    }

    #[tokio::test]
    async fn test_ipv6_protection() {
        // Block IPv6 loopback
        assert!(validate_url_safe("http://[::1]/admin").await.is_err());

        // Block IPv6 link-local (fe80::/10)
        assert!(validate_url_safe("http://[fe80::1]/test").await.is_err());

        // Block IPv6 unique local (fc00::/7)
        assert!(validate_url_safe("http://[fc00::1]/test").await.is_err());
        assert!(validate_url_safe("http://[fd00::1]/test").await.is_err());
    }

    #[test]
    fn test_private_ip_detection() {
        // Test all private ranges
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 255, 255, 255))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            192, 168, 255, 255
        ))));

        // Test loopback
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            127, 255, 255, 255
        ))));

        // Test link-local
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            169, 254, 255, 255
        ))));

        // Test broadcast
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(
            255, 255, 255, 255
        ))));

        // Test reserved
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));

        // Test public IPs (should NOT be private)
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)))); // example.com
    }

    #[test]
    fn test_hostname_blocklist() {
        // Test exact matches
        assert!(is_blocked_hostname("localhost"));
        assert!(is_blocked_hostname("metadata"));
        assert!(is_blocked_hostname("metadata.google.internal"));

        // Test case insensitivity
        assert!(is_blocked_hostname("LOCALHOST"));
        assert!(is_blocked_hostname("MetaData"));

        // Test subdomain matching
        assert!(is_blocked_hostname("sub.metadata"));

        // Test that legitimate domains are not blocked
        assert!(!is_blocked_hostname("example.com"));
        assert!(!is_blocked_hostname("google.com"));
        assert!(!is_blocked_hostname("localhosting.com")); // Should not match due to exact matching logic
    }

    #[tokio::test]
    async fn test_cloud_metadata_protection() {
        // AWS metadata endpoints
        assert!(
            validate_url_safe("http://169.254.169.254/latest/meta-data/")
                .await
                .is_err()
        );
        assert!(validate_url_safe("http://instance-data/latest/meta-data/")
            .await
            .is_err());

        // GCP metadata endpoints
        assert!(
            validate_url_safe("http://metadata.google.internal/computeMetadata/v1/")
                .await
                .is_err()
        );
        assert!(validate_url_safe("http://metadata/computeMetadata/v1/")
            .await
            .is_err());
    }
}
