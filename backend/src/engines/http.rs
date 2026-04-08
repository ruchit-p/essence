use crate::{
    engines::{RawScrapeResult, ScrapeEngine},
    error::{Result, ScrapeError},
    types::ScrapeRequest,
    utils::{
        dns_cache::DnsCache,
        retry::{retry_with_backoff, RetryStrategy},
        url_rewrites::rewrite_url,
        user_agents::random_user_agent,
    },
};
use async_trait::async_trait;
use encoding_rs::Encoding;
use reqwest::{redirect::Policy, Client};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub struct HttpEngine {
    client: Client,
    dns_cache: DnsCache,
}

impl HttpEngine {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Essence/0.1.0; +https://essence.foundation)")
            .gzip(true)
            .brotli(true)
            .redirect(Policy::limited(10))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .map_err(|e| ScrapeError::Internal(format!("Failed to build HTTP client: {}", e)))?;

        let dns_cache = DnsCache::new()?;

        Ok(Self { client, dns_cache })
    }

    pub fn with_timeout(timeout_ms: u64) -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Essence/0.1.0; +https://essence.foundation)")
            .gzip(true)
            .brotli(true)
            .redirect(Policy::limited(10))
            .timeout(Duration::from_millis(timeout_ms))
            .connect_timeout(Duration::from_millis(timeout_ms.min(10000)))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60))
            .build()
            .map_err(|e| ScrapeError::Internal(format!("Failed to build HTTP client: {}", e)))?;

        let dns_cache = DnsCache::new()?;

        Ok(Self { client, dns_cache })
    }

    pub fn with_options(timeout_ms: u64, skip_tls_verification: bool) -> Result<Self> {
        let mut builder = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Essence/0.1.0; +https://essence.foundation)")
            .gzip(true)
            .brotli(true)
            .redirect(Policy::limited(10))
            .timeout(Duration::from_millis(timeout_ms))
            .connect_timeout(Duration::from_millis(timeout_ms.min(10000)))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(60));

        if skip_tls_verification {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder
            .build()
            .map_err(|e| ScrapeError::Internal(format!("Failed to build HTTP client: {}", e)))?;

        let dns_cache = DnsCache::new()?;

        Ok(Self { client, dns_cache })
    }

    /// Get DNS cache statistics
    pub async fn dns_stats(&self) -> crate::utils::dns_cache::CacheStats {
        self.dns_cache.stats().await
    }

    /// Clear DNS cache
    pub async fn clear_dns_cache(&self) {
        self.dns_cache.clear().await
    }
}

impl Default for HttpEngine {
    fn default() -> Self {
        Self::new().expect("Failed to create default HTTP engine")
    }
}

#[async_trait]
impl ScrapeEngine for HttpEngine {
    async fn scrape(&self, request: &ScrapeRequest) -> Result<RawScrapeResult> {
        let start = Instant::now();

        // Use aggressive retry strategy for HTTP requests to handle network failures
        // This will retry up to 5 times with exponential backoff starting at 200ms
        let retry_config = RetryStrategy::Aggressive.to_config();

        debug!(
            "HTTP engine starting request to {} with retry config: max_retries={}, initial_delay={:?}",
            request.url,
            retry_config.max_retries,
            retry_config.initial_interval
        );

        // Wrap the scrape operation in retry logic
        let result =
            retry_with_backoff(|| async { self.scrape_once(request).await }, &retry_config).await;

        // Track duration
        let duration = start.elapsed().as_secs_f64();

        // Log result
        if let Err(ref e) = result {
            warn!(
                "HTTP engine failed for {} after {:.2}s: {}",
                request.url, duration, e
            );
        } else {
            info!(
                "HTTP engine succeeded for {} in {:.2}s",
                request.url, duration
            );
        }

        result
    }
}

impl HttpEngine {
    /// Perform a single scrape attempt without retry logic
    async fn scrape_once(&self, request: &ScrapeRequest) -> Result<RawScrapeResult> {
        let request_start = Instant::now();

        // Rewrite URL if needed (e.g., Google Docs → export URL)
        let url_str = rewrite_url(&request.url);

        // Validate URL
        let url = reqwest::Url::parse(&url_str).map_err(|e| {
            warn!("URL parsing failed for '{}': {}", url_str, e);
            ScrapeError::InvalidUrl(format!("Invalid URL: {}", e))
        })?;

        debug!("Starting HTTP request to: {}", url);

        // Pre-flight DNS resolution for metrics and early failure detection
        if let Some(host) = url.host_str() {
            let dns_start = Instant::now();
            match self.dns_cache.lookup(host).await {
                Ok(ips) => {
                    let dns_elapsed = dns_start.elapsed();
                    let stats = self.dns_cache.stats().await;
                    debug!(
                        "DNS resolved {} to {} IPs in {:.2}ms (cache hit rate: {:.1}%)",
                        host,
                        ips.len(),
                        dns_elapsed.as_secs_f64() * 1000.0,
                        stats.hit_rate() * 100.0
                    );
                }
                Err(e) => {
                    // DNS resolution failed - this will likely fail the HTTP request too
                    // but we let reqwest handle it for consistency
                    debug!("DNS pre-flight resolution failed for {}: {}", host, e);
                }
            }
        }

        // Build request with custom headers
        let mut req_builder = self.client.get(url.clone());

        // Use random user agent if not provided in request headers
        let user_agent = request
            .headers
            .get("User-Agent")
            .or_else(|| request.headers.get("user-agent"))
            .cloned()
            .unwrap_or_else(|| random_user_agent().to_string());

        debug!("Using User-Agent: {}", user_agent);
        req_builder = req_builder
            .header("User-Agent", &user_agent)
            // Browser-like headers to reduce anti-bot detection
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Upgrade-Insecure-Requests", "1")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1");

        // Add custom headers (overriding defaults if specified)
        for (key, value) in &request.headers {
            if key.to_lowercase() != "user-agent" {
                req_builder = req_builder.header(key, value);
            }
        }

        // Send request
        let send_start = Instant::now();
        let response = req_builder.send().await.map_err(|e| {
            let elapsed = send_start.elapsed();

            // Classify the error for better diagnostics
            if e.is_timeout() {
                warn!(
                    "Request timeout for {} after {:.2}s",
                    url,
                    elapsed.as_secs_f64()
                );
                ScrapeError::Timeout
            } else if e.is_connect() {
                warn!(
                    "Connection failed for {} after {:.2}ms: {}",
                    url,
                    elapsed.as_secs_f64() * 1000.0,
                    e
                );
                ScrapeError::RequestFailed(e)
            } else if e.is_request() {
                warn!(
                    "Request error for {} after {:.2}ms: {}",
                    url,
                    elapsed.as_secs_f64() * 1000.0,
                    e
                );
                ScrapeError::RequestFailed(e)
            } else {
                warn!(
                    "Network error for {} after {:.2}ms: {}",
                    url,
                    elapsed.as_secs_f64() * 1000.0,
                    e
                );
                ScrapeError::RequestFailed(e)
            }
        })?;

        let request_elapsed = request_start.elapsed();
        info!(
            "HTTP request to {} completed in {:.2}ms (status: {})",
            url,
            request_elapsed.as_secs_f64() * 1000.0,
            response.status()
        );

        // Extract metadata
        let final_url = response.url().to_string();
        let status_code = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Collect headers
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|val| (k.to_string(), val.to_string())))
            .collect();

        // Get HTML content with charset detection
        let bytes = response.bytes().await.map_err(ScrapeError::RequestFailed)?;

        // Detect charset from Content-Type or HTML meta tag
        let encoding = detect_charset(&bytes, content_type.as_deref());

        // Decode with detected charset
        let (html, _, had_errors) = encoding.decode(&bytes);
        if had_errors {
            debug!(
                "Charset decoding had errors for {}, encoding: {}",
                url,
                encoding.name()
            );
        }
        let html = html.into_owned();

        // Check response size limit
        let max_response_size_mb = std::env::var("MAX_RESPONSE_SIZE_MB")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(50);

        let max_size_bytes = max_response_size_mb * 1024 * 1024;

        if html.len() > max_size_bytes {
            return Err(ScrapeError::ResourceLimit(format!(
                "Response too large: {:.2}MB > {}MB",
                html.len() as f64 / (1024.0 * 1024.0),
                max_response_size_mb
            )));
        }

        Ok(RawScrapeResult {
            url: final_url,
            status_code,
            content_type,
            html,
            headers,
        })
    }
}

/// Detect charset from Content-Type header or HTML meta tag
fn detect_charset(bytes: &[u8], content_type: Option<&str>) -> &'static Encoding {
    // 1. Try Content-Type header first
    if let Some(ct) = content_type {
        if let Some(charset) = extract_charset_from_header(ct) {
            if let Some(encoding) = Encoding::for_label(charset.as_bytes()) {
                debug!("Detected charset from Content-Type: {}", charset);
                return encoding;
            }
        }
    }

    // 2. Try HTML meta tag (look in first 2KB)
    let preview = std::str::from_utf8(&bytes[..bytes.len().min(2048)]).unwrap_or("");
    if let Some(charset) = extract_charset_from_meta(preview) {
        if let Some(encoding) = Encoding::for_label(charset.as_bytes()) {
            debug!("Detected charset from meta tag: {}", charset);
            return encoding;
        }
    }

    // 3. Default to UTF-8
    debug!("No charset detected, using UTF-8");
    encoding_rs::UTF_8
}

/// Extract charset from Content-Type header
fn extract_charset_from_header(content_type: &str) -> Option<String> {
    // Parse "text/html; charset=UTF-8"
    content_type
        .split(';')
        .find(|part| part.trim().starts_with("charset="))
        .and_then(|charset_part| {
            charset_part
                .trim()
                .strip_prefix("charset=")
                .map(|s| s.trim().trim_matches('"').to_string())
        })
}

/// Extract charset from HTML meta tag
fn extract_charset_from_meta(html: &str) -> Option<String> {
    // Match: <meta charset="UTF-8"> or <meta http-equiv="Content-Type" content="text/html; charset=UTF-8">
    use regex::Regex;

    // Try <meta charset="..."> first
    if let Ok(re) = Regex::new(r#"(?i)<meta\s+[^>]*charset\s*=\s*["']?([^"'\s/>]+)"#) {
        if let Some(caps) = re.captures(html) {
            if let Some(m) = caps.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }

    // Try <meta http-equiv="Content-Type" content="...; charset=...">
    if let Ok(re) = Regex::new(
        r#"(?i)<meta\s+http-equiv\s*=\s*["']?content-type["']?\s+content\s*=\s*["'][^"']*charset=([^"'\s;]+)"#,
    ) {
        if let Some(caps) = re.captures(html) {
            if let Some(m) = caps.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = HttpEngine::new();
        assert!(engine.is_ok());
    }

    #[test]
    fn test_engine_with_timeout() {
        let engine = HttpEngine::with_timeout(5000);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_engine_with_options() {
        let engine = HttpEngine::with_options(5000, true);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_detect_charset_from_content_type() {
        let encoding = detect_charset(b"", Some("text/html; charset=EUC-JP"));
        assert_eq!(encoding.name(), "EUC-JP");
    }

    #[test]
    fn test_detect_charset_from_meta() {
        let html = r#"<html><head><meta charset="ISO-8859-1"></head></html>"#;
        let encoding = detect_charset(html.as_bytes(), None);
        // ISO-8859-1 maps to windows-1252 in encoding_rs
        assert_eq!(encoding.name(), "windows-1252");
    }

    #[test]
    fn test_detect_charset_from_meta_http_equiv() {
        let html = r#"<html><head><meta http-equiv="Content-Type" content="text/html; charset=GB2312"></head></html>"#;
        let encoding = detect_charset(html.as_bytes(), None);
        assert_eq!(encoding.name(), "GBK");
    }

    #[test]
    fn test_detect_charset_utf8_default() {
        let encoding = detect_charset(b"", None);
        assert_eq!(encoding.name(), "UTF-8");
    }

    #[test]
    fn test_extract_charset_from_header() {
        assert_eq!(
            extract_charset_from_header("text/html; charset=UTF-8"),
            Some("UTF-8".to_string())
        );
        assert_eq!(
            extract_charset_from_header("text/html; charset=\"ISO-8859-1\""),
            Some("ISO-8859-1".to_string())
        );
        assert_eq!(extract_charset_from_header("text/html"), None);
    }

    #[test]
    fn test_extract_charset_from_meta() {
        assert_eq!(
            extract_charset_from_meta(r#"<meta charset="UTF-8">"#),
            Some("UTF-8".to_string())
        );
        assert_eq!(
            extract_charset_from_meta(
                r#"<meta http-equiv="Content-Type" content="text/html; charset=ISO-8859-1">"#
            ),
            Some("ISO-8859-1".to_string())
        );
        assert_eq!(extract_charset_from_meta("<html></html>"), None);
    }
}
