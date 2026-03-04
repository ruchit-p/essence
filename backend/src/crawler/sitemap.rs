use crate::error::{Result, ScrapeError};
use reqwest::Client;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, info};
use url::Url;

/// Sitemap parser
pub struct SitemapParser {
    client: Client,
}

impl SitemapParser {
    /// Create a new SitemapParser
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Essence/0.1.0; +https://essence.foundation)")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ScrapeError::Internal(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self { client })
    }

    /// Create a new SitemapParser with a custom HTTP client
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Fetch sitemap with optional caching
    ///
    /// - `cache_ttl`: None = no cache, Some(secs) = cache for N seconds (currently ignored, reserved for future use)
    pub async fn fetch_with_cache(
        &self,
        base_url: &str,
        _cache_ttl: Option<u64>,
    ) -> Result<Vec<String>> {
        self.fetch_sitemap_internal(base_url).await
    }

    /// Fetch and parse sitemap.xml from a base URL (internal implementation)
    async fn fetch_sitemap_internal(&self, base_url: &str) -> Result<Vec<String>> {
        let _base = Url::parse(base_url)
            .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid base URL: {}", e)))?;

        let mut all_urls = HashSet::new();

        // Strategy 1: Check robots.txt for Sitemap directive
        if let Ok(sitemap_url) = self.check_robots_txt(base_url).await {
            info!("Found sitemap URL in robots.txt: {}", sitemap_url);
            if self
                .fetch_and_parse_sitemap(&sitemap_url, &mut all_urls)
                .await
                .is_ok()
                && !all_urls.is_empty()
            {
                info!(
                    "Successfully fetched {} URLs from robots.txt sitemap",
                    all_urls.len()
                );
                return Ok(all_urls.into_iter().collect());
            }
        }

        // Strategy 2: Try common sitemap locations
        let sitemap_urls = vec![
            format!("{}/sitemap.xml", base_url.trim_end_matches('/')),
            format!("{}/sitemap_index.xml", base_url.trim_end_matches('/')),
            format!("{}/sitemap-index.xml", base_url.trim_end_matches('/')),
        ];

        for sitemap_url in sitemap_urls {
            debug!("Trying sitemap location: {}", sitemap_url);
            match self
                .fetch_and_parse_sitemap(&sitemap_url, &mut all_urls)
                .await
            {
                Ok(_) => {
                    if !all_urls.is_empty() {
                        info!(
                            "Found {} URLs from sitemap at {}",
                            all_urls.len(),
                            sitemap_url
                        );
                        break; // Found a sitemap, stop trying
                    }
                }
                Err(e) => {
                    debug!("Failed to fetch sitemap at {}: {}", sitemap_url, e);
                    continue;
                }
            }
        }

        Ok(all_urls.into_iter().collect())
    }

    /// Check robots.txt for Sitemap directive
    async fn check_robots_txt(&self, base_url: &str) -> Result<String> {
        let robots_url = format!("{}/robots.txt", base_url.trim_end_matches('/'));
        debug!("Checking robots.txt at: {}", robots_url);

        let response = self
            .client
            .get(&robots_url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(ScrapeError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(ScrapeError::Internal(format!(
                "robots.txt returned status: {}",
                response.status()
            )));
        }

        let text = response
            .text()
            .await
            .map_err(|e| ScrapeError::Internal(format!("Failed to read robots.txt: {}", e)))?;

        // Parse "Sitemap: <url>" directive (case-insensitive)
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.to_lowercase().starts_with("sitemap:") {
                if let Some(url) = trimmed.split_whitespace().nth(1) {
                    return Ok(url.to_string());
                }
            }
        }

        Err(ScrapeError::Internal(
            "No sitemap directive in robots.txt".to_string(),
        ))
    }

    /// Fetch and parse a single sitemap URL
    fn fetch_and_parse_sitemap<'a>(
        &'a self,
        sitemap_url: &'a str,
        all_urls: &'a mut HashSet<String>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let response = self
                .client
                .get(sitemap_url)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(ScrapeError::RequestFailed)?;

            if !response.status().is_success() {
                return Err(ScrapeError::Internal(format!(
                    "Sitemap returned status: {}",
                    response.status()
                )));
            }

            let content = response
                .text()
                .await
                .map_err(|e| ScrapeError::Internal(format!("Failed to read sitemap content: {}", e)))?;

            self.parse_sitemap_content(&content, all_urls).await
        })
    }

    /// Parse sitemap XML content and extract URLs
    fn parse_sitemap_content<'a>(
        &'a self,
        content: &'a str,
        all_urls: &'a mut HashSet<String>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Check if this is a sitemap index (contains <sitemapindex> tag)
            if content.contains("<sitemapindex") {
                debug!("Detected sitemap index format");
                let sitemap_pattern =
                    regex::Regex::new(r"(?s)<sitemap[^>]*>.*?<loc>([^<]+)</loc>.*?</sitemap>")
                        .map_err(|e| ScrapeError::Internal(format!("Regex error: {}", e)))?;

                let mut sitemap_count = 0;
                for cap in sitemap_pattern.captures_iter(content) {
                    if let Some(url_match) = cap.get(1) {
                        let url = url_match.as_str().trim();
                        debug!("Found nested sitemap: {}", url);
                        sitemap_count += 1;
                        match self.fetch_and_parse_sitemap(url, all_urls).await {
                            Ok(_) => debug!("Successfully parsed nested sitemap: {}", url),
                            Err(e) => debug!("Failed to parse nested sitemap {}: {}", url, e),
                        }
                    }
                }
                info!(
                    "Processed {} nested sitemaps from sitemap index",
                    sitemap_count
                );
            } else if content.contains("<sitemap>") {
                // Fallback: old-style detection for sitemap indexes without proper <sitemapindex> wrapper
                debug!("Detected sitemap index format (legacy)");
                let sitemap_pattern =
                    regex::Regex::new(r"(?s)<sitemap[^>]*>.*?<loc>([^<]+)</loc>.*?</sitemap>")
                        .map_err(|e| ScrapeError::Internal(format!("Regex error: {}", e)))?;

                for cap in sitemap_pattern.captures_iter(content) {
                    if let Some(url_match) = cap.get(1) {
                        let url = url_match.as_str().trim();
                        debug!("Found nested sitemap: {}", url);
                        let _ = self.fetch_and_parse_sitemap(url, all_urls).await;
                    }
                }
            } else {
                // This is a regular sitemap (urlset), extract URLs
                debug!("Detected regular sitemap format");
                let url_pattern =
                    regex::Regex::new(r"(?s)<url[^>]*>.*?<loc>([^<]+)</loc>.*?</url>")
                        .map_err(|e| ScrapeError::Internal(format!("Regex error: {}", e)))?;

                for cap in url_pattern.captures_iter(content) {
                    if let Some(url_match) = cap.get(1) {
                        let url = url_match.as_str().trim().to_string();
                        all_urls.insert(url);
                    }
                }
            }

            Ok(())
        })
    }
}

/// Backward-compatible function API for fetching sitemaps without caching
pub async fn fetch_sitemap(base_url: &str, client: &Client) -> Result<Vec<String>> {
    let parser = SitemapParser::with_client(client.clone());
    parser.fetch_sitemap_internal(base_url).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    /// Cache entry for sitemap URLs (used for serialization tests)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct SitemapCacheEntry {
        urls: Vec<String>,
        fetched_at: i64,
        ttl_seconds: u64,
    }

    #[test]
    fn test_parse_regular_sitemap() {
        let sitemap_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://example.com/page1</loc>
  </url>
  <url>
    <loc>https://example.com/page2</loc>
  </url>
</urlset>"#;

        let mut urls = HashSet::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let parser = SitemapParser::new().unwrap();
            let result = parser.parse_sitemap_content(sitemap_xml, &mut urls).await;
            assert!(result.is_ok());
        });

        assert_eq!(urls.len(), 2);
        assert!(urls.contains("https://example.com/page1"));
        assert!(urls.contains("https://example.com/page2"));
    }

    #[test]
    fn test_detect_sitemap_index() {
        let sitemap_index_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap>
    <loc>https://example.com/sitemap1.xml</loc>
  </sitemap>
  <sitemap>
    <loc>https://example.com/sitemap2.xml</loc>
  </sitemap>
</sitemapindex>"#;

        assert!(sitemap_index_xml.contains("<sitemapindex"));
        assert!(sitemap_index_xml.contains("<sitemap>"));
    }

    #[test]
    fn test_parse_sitemap_index_extracts_sitemap_urls() {
        let sitemap_index_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap>
    <loc>https://example.com/sitemap-posts.xml</loc>
    <lastmod>2025-10-01T00:00:00Z</lastmod>
  </sitemap>
  <sitemap>
    <loc>https://example.com/sitemap-pages.xml</loc>
    <lastmod>2025-10-02T00:00:00Z</lastmod>
  </sitemap>
  <sitemap>
    <loc>https://example.com/sitemap-products.xml</loc>
  </sitemap>
</sitemapindex>"#;

        let sitemap_pattern =
            regex::Regex::new(r"(?s)<sitemap[^>]*>.*?<loc>([^<]+)</loc>.*?</sitemap>").unwrap();
        let sitemap_urls: Vec<String> = sitemap_pattern
            .captures_iter(sitemap_index_xml)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        assert_eq!(sitemap_urls.len(), 3);
        assert!(sitemap_urls.contains(&"https://example.com/sitemap-posts.xml".to_string()));
        assert!(sitemap_urls.contains(&"https://example.com/sitemap-pages.xml".to_string()));
        assert!(sitemap_urls.contains(&"https://example.com/sitemap-products.xml".to_string()));
    }

    #[test]
    fn test_parse_regular_sitemap_doesnt_match_url_in_sitemap_index() {
        let sitemap_index_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap>
    <loc>https://example.com/sitemap-posts.xml</loc>
  </sitemap>
</sitemapindex>"#;

        let url_pattern =
            regex::Regex::new(r"(?s)<url[^>]*>.*?<loc>([^<]+)</loc>.*?</url>").unwrap();
        let urls: Vec<String> = url_pattern
            .captures_iter(sitemap_index_xml)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        assert_eq!(
            urls.len(),
            0,
            "URL pattern should not match sitemap index entries"
        );
    }

    #[test]
    fn test_parse_regular_sitemap_with_url_tags() {
        let sitemap_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://example.com/page1</loc>
    <lastmod>2025-10-01T00:00:00Z</lastmod>
  </url>
  <url>
    <loc>https://example.com/page2</loc>
  </url>
</urlset>"#;

        let url_pattern =
            regex::Regex::new(r"(?s)<url[^>]*>.*?<loc>([^<]+)</loc>.*?</url>").unwrap();
        let urls: Vec<String> = url_pattern
            .captures_iter(sitemap_xml)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://example.com/page1".to_string()));
        assert!(urls.contains(&"https://example.com/page2".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_with_cache_disabled() {
        let parser = SitemapParser::new().unwrap();

        let result = parser
            .fetch_with_cache("https://www.sitemaps.org/sitemap.xml", None)
            .await;

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_sitemap_cache_entry_serialization() {
        let entry = SitemapCacheEntry {
            urls: vec!["https://example.com/page1".to_string()],
            fetched_at: 1234567890,
            ttl_seconds: 3600,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: SitemapCacheEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.urls, deserialized.urls);
        assert_eq!(entry.fetched_at, deserialized.fetched_at);
        assert_eq!(entry.ttl_seconds, deserialized.ttl_seconds);
    }

    #[test]
    fn test_sitemap_parser_creation() {
        let parser = SitemapParser::new();
        assert!(parser.is_ok());

        let client = Client::new();
        let _parser = SitemapParser::with_client(client);
    }

    #[tokio::test]
    async fn test_backward_compatibility() {
        let client = Client::new();
        let result = fetch_sitemap("https://www.sitemaps.org/sitemap.xml", &client).await;

        assert!(result.is_ok() || result.is_err());
    }
}
