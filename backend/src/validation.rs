use crate::error::ScrapeError;
use crate::types::{
    CrawlRequest, ExtractRequest, LlmsTxtRequest, MapRequest, ScrapeRequest, SearchRequest,
};
use crate::utils::ssrf_protection;
use scraper::Selector;
use std::time::Duration;
use url::Url;

// Size limits (in bytes)
const MAX_URL_LENGTH: usize = 2048;
const MAX_HEADERS_COUNT: usize = 50;
const MAX_HEADER_VALUE_LENGTH: usize = 4096;
const MAX_ACTIONS_COUNT: usize = 20;
const MAX_CSS_SELECTOR_LENGTH: usize = 1000;
const MAX_TIMEOUT_MS: u64 = 300_000; // 5 minutes
const MAX_CRAWL_TIMEOUT_MS: u64 = 300_000; // 5 minutes for crawls
const MAX_CRAWL_LIMIT: u32 = 10_000;
const MAX_MAP_LIMIT: u32 = 100_000;
const MAX_SEARCH_LIMIT: u32 = 100;

/// Validate a URL (including SSRF protection)
pub async fn validate_url(url: &str) -> Result<(), ScrapeError> {
    if url.is_empty() {
        return Err(ScrapeError::InvalidUrl("URL cannot be empty".to_string()));
    }

    if url.len() > MAX_URL_LENGTH {
        return Err(ScrapeError::InvalidUrl(format!(
            "URL too long: {} > {} characters",
            url.len(),
            MAX_URL_LENGTH
        )));
    }

    Url::parse(url).map_err(|e| ScrapeError::InvalidUrl(format!("Invalid URL: {}", e)))?;

    // SSRF protection: check for private IPs and DNS rebinding attacks
    ssrf_protection::validate_url_safe(url).await?;

    Ok(())
}

/// Validate a CSS selector
pub fn validate_css_selector(selector: &str) -> Result<(), ScrapeError> {
    if selector.is_empty() {
        return Ok(());
    }

    if selector.len() > MAX_CSS_SELECTOR_LENGTH {
        return Err(ScrapeError::InvalidRequest(format!(
            "CSS selector too long: {} > {} characters",
            selector.len(),
            MAX_CSS_SELECTOR_LENGTH
        )));
    }

    // Check for dangerous patterns
    let dangerous_patterns = [
        "<script",
        "javascript:",
        "eval(",
        "onclick=",
        "onerror=",
        "onload=",
    ];

    for pattern in &dangerous_patterns {
        if selector.to_lowercase().contains(pattern) {
            return Err(ScrapeError::InvalidRequest(format!(
                "Invalid CSS selector: contains dangerous pattern '{}'",
                pattern
            )));
        }
    }

    // Validate it parses correctly
    Selector::parse(selector).map_err(|e| {
        ScrapeError::InvalidRequest(format!("Invalid CSS selector syntax: {:?}", e))
    })?;

    Ok(())
}

/// Validate scrape request
pub async fn validate_scrape_request(req: &ScrapeRequest) -> Result<(), ScrapeError> {
    // URL validation
    validate_url(&req.url).await?;

    // Timeout validation
    if req.timeout > MAX_TIMEOUT_MS {
        return Err(ScrapeError::InvalidRequest(format!(
            "Timeout too large: {}ms > {}ms",
            req.timeout, MAX_TIMEOUT_MS
        )));
    }

    // Headers validation
    if req.headers.len() > MAX_HEADERS_COUNT {
        return Err(ScrapeError::InvalidRequest(format!(
            "Too many headers: {} > {}",
            req.headers.len(),
            MAX_HEADERS_COUNT
        )));
    }

    for (key, value) in &req.headers {
        if value.len() > MAX_HEADER_VALUE_LENGTH {
            return Err(ScrapeError::InvalidRequest(format!(
                "Header '{}' value too long: {} > {} characters",
                key,
                value.len(),
                MAX_HEADER_VALUE_LENGTH
            )));
        }
    }

    // Actions validation
    if req.actions.len() > MAX_ACTIONS_COUNT {
        return Err(ScrapeError::InvalidRequest(format!(
            "Too many browser actions: {} > {}",
            req.actions.len(),
            MAX_ACTIONS_COUNT
        )));
    }

    // Selector validation
    if let Some(ref selector) = req.wait_for_selector {
        validate_css_selector(selector)?;
    }

    for tag in &req.include_tags {
        validate_css_selector(tag)?;
    }

    for tag in &req.exclude_tags {
        validate_css_selector(tag)?;
    }

    // Format validation
    let valid_formats = [
        "markdown",
        "html",
        "rawHtml",
        "links",
        "images",
        "screenshot",
    ];

    for format in &req.formats {
        if !valid_formats.contains(&format.as_str()) {
            return Err(ScrapeError::UnsupportedFormat(format.clone()));
        }
    }

    Ok(())
}

/// Validate map request
pub async fn validate_map_request(req: &MapRequest) -> Result<(), ScrapeError> {
    validate_url(&req.url).await?;

    if let Some(limit) = req.limit {
        if limit > MAX_MAP_LIMIT {
            return Err(ScrapeError::InvalidRequest(format!(
                "Map limit too large: {} > {}",
                limit, MAX_MAP_LIMIT
            )));
        }
    }

    Ok(())
}

/// Validate crawl request
pub async fn validate_crawl_request(req: &CrawlRequest) -> Result<(), ScrapeError> {
    validate_url(&req.url).await?;

    if req.limit > MAX_CRAWL_LIMIT {
        return Err(ScrapeError::InvalidRequest(format!(
            "Crawl limit too large: {} > {}",
            req.limit, MAX_CRAWL_LIMIT
        )));
    }

    Ok(())
}

/// Validate search request
pub fn validate_search_request(req: &SearchRequest) -> Result<(), ScrapeError> {
    if req.query.is_empty() {
        return Err(ScrapeError::InvalidRequest(
            "Search query cannot be empty".to_string(),
        ));
    }

    if req.limit > MAX_SEARCH_LIMIT {
        return Err(ScrapeError::InvalidRequest(format!(
            "Search limit too large: {} > {}",
            req.limit, MAX_SEARCH_LIMIT
        )));
    }

    Ok(())
}

const MAX_EXTRACT_URLS: usize = 10;

/// Validate extract request
pub async fn validate_extract_request(req: &ExtractRequest) -> Result<(), ScrapeError> {
    if req.urls.is_empty() {
        return Err(ScrapeError::InvalidRequest(
            "At least one URL is required".to_string(),
        ));
    }

    if req.urls.len() > MAX_EXTRACT_URLS {
        return Err(ScrapeError::InvalidRequest(format!(
            "Too many URLs: {} > {}",
            req.urls.len(),
            MAX_EXTRACT_URLS
        )));
    }

    for url in &req.urls {
        validate_url(url).await?;
    }

    // Validate mode
    if !matches!(req.mode.as_str(), "auto" | "llm" | "css") {
        return Err(ScrapeError::InvalidRequest(format!(
            "Invalid extraction mode '{}'. Must be 'auto', 'llm', or 'css'",
            req.mode
        )));
    }

    // LLM mode requires credentials
    if req.mode == "llm" && req.llm_base_url.is_none() {
        return Err(ScrapeError::InvalidRequest(
            "LLM mode requires 'llmBaseUrl' to be set".to_string(),
        ));
    }

    // CSS mode requires selectors
    if req.mode == "css" && req.selectors.is_none() {
        return Err(ScrapeError::InvalidRequest(
            "CSS mode requires 'selectors' field with CSS selector mappings".to_string(),
        ));
    }

    // Validate CSS selectors if provided
    if let Some(selectors) = &req.selectors {
        for (field, selector) in selectors {
            validate_css_selector(selector).map_err(|e| {
                ScrapeError::InvalidRequest(format!(
                    "Invalid CSS selector for field '{}': {}",
                    field, e
                ))
            })?;
        }
    }

    // Validate timeout
    if req.timeout > MAX_TIMEOUT_MS {
        return Err(ScrapeError::InvalidRequest(format!(
            "Timeout too large: {}ms > {}ms",
            req.timeout, MAX_TIMEOUT_MS
        )));
    }

    Ok(())
}

const MAX_LLMSTXT_URLS: u32 = 500;

/// Validate llms.txt request
pub async fn validate_llmstxt_request(req: &LlmsTxtRequest) -> Result<(), ScrapeError> {
    validate_url(&req.url).await?;

    if req.max_urls > MAX_LLMSTXT_URLS {
        return Err(ScrapeError::InvalidRequest(format!(
            "max_urls too large: {} > {}",
            req.max_urls, MAX_LLMSTXT_URLS
        )));
    }

    if req.max_concurrent_scrapes == 0 || req.max_concurrent_scrapes > 50 {
        return Err(ScrapeError::InvalidRequest(
            "max_concurrent_scrapes must be between 1 and 50".to_string(),
        ));
    }

    Ok(())
}

/// Get timeout duration for crawl operations
pub fn get_crawl_timeout() -> Duration {
    Duration::from_millis(MAX_CRAWL_TIMEOUT_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_url() {
        assert!(validate_url("https://example.com").await.is_ok());
        assert!(validate_url("").await.is_err());
        assert!(validate_url(&"a".repeat(3000)).await.is_err());
        assert!(validate_url("not-a-url").await.is_err());
    }

    #[test]
    fn test_validate_css_selector() {
        assert!(validate_css_selector("div.class").is_ok());
        assert!(validate_css_selector("#id").is_ok());
        assert!(validate_css_selector("<script>alert('xss')</script>").is_err());
        assert!(validate_css_selector("javascript:void(0)").is_err());
        assert!(validate_css_selector(&"a".repeat(2000)).is_err());
    }

    #[tokio::test]
    async fn test_validate_scrape_request() {
        let valid_req = ScrapeRequest {
            url: "https://example.com".to_string(),
            formats: vec!["markdown".to_string()],
            headers: Default::default(),
            include_tags: vec![],
            exclude_tags: vec![],
            only_main_content: true,
            timeout: 30000,
            wait_for: 0,
            remove_base64_images: true,
            skip_tls_verification: false,
            engine: "auto".to_string(),
            wait_for_selector: None,
            actions: vec![],
            screenshot: false,
            screenshot_format: "png".to_string(),
        };

        assert!(validate_scrape_request(&valid_req).await.is_ok());

        // Test timeout validation
        let mut invalid_req = valid_req.clone();
        invalid_req.timeout = 400_000; // > 5 minutes
        assert!(validate_scrape_request(&invalid_req).await.is_err());

        // Test format validation
        let mut invalid_req = valid_req.clone();
        invalid_req.formats = vec!["invalid_format".to_string()];
        assert!(validate_scrape_request(&invalid_req).await.is_err());
    }
}
