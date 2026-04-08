use crate::{
    engines::{http::HttpEngine, ScrapeEngine},
    error::{Result, ScrapeError},
    format,
    types::{Document, ScrapeRequest, SearchResult},
    utils::retry::{retry_with_backoff, RetryStrategy},
};
use scraper::{Html, Selector};
use tracing::{info, warn};

/// Search provider interface
pub struct SearchProvider {
    http_client: reqwest::Client,
}

impl SearchProvider {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; Essence/0.1.0; +https://essence.foundation)")
            .build()
            .map_err(|e| ScrapeError::Internal(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            http_client: client,
        })
    }

    /// Search DuckDuckGo and return results
    pub async fn search_duckduckgo(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>> {
        // Use conservative retry strategy for search (less aggressive)
        let retry_config = RetryStrategy::Conservative.to_config();

        // Wrap the search operation in retry logic
        retry_with_backoff(
            || async { self.search_duckduckgo_once(query, limit).await },
            &retry_config,
        )
        .await
    }

    /// Perform a single DuckDuckGo search attempt without retry logic
    async fn search_duckduckgo_once(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>> {
        info!("Searching DuckDuckGo for: {}", query);

        // DuckDuckGo HTML search URL
        let search_url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        // Fetch search results page
        let response = self
            .http_client
            .get(&search_url)
            .send()
            .await
            .map_err(ScrapeError::RequestFailed)?;

        let html_content = response.text().await.map_err(ScrapeError::RequestFailed)?;

        // Parse HTML
        let document = Html::parse_document(&html_content);

        // DuckDuckGo HTML selectors
        let result_selector = Selector::parse(".result").expect("valid CSS selector");
        let title_selector = Selector::parse(".result__a").expect("valid CSS selector");
        let snippet_selector = Selector::parse(".result__snippet").expect("valid CSS selector");

        let mut results = Vec::new();

        for result_elem in document.select(&result_selector) {
            if results.len() >= limit as usize {
                break;
            }

            // Extract title and URL
            let title_elem = result_elem.select(&title_selector).next();
            let snippet_elem = result_elem.select(&snippet_selector).next();

            if let Some(title_node) = title_elem {
                let title = title_node
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                let url = title_node.value().attr("href").unwrap_or("").to_string();

                // DuckDuckGo uses redirect URLs, extract the actual URL
                let actual_url = extract_url_from_duckduckgo(&url);

                let snippet = snippet_elem
                    .map(|s| s.text().collect::<Vec<_>>().join(" ").trim().to_string())
                    .unwrap_or_default();

                if !actual_url.is_empty() && actual_url.starts_with("http") {
                    results.push(SearchResult {
                        title,
                        url: actual_url,
                        snippet,
                        content: None,
                    });
                }
            }
        }

        info!("Found {} search results", results.len());
        Ok(results)
    }

    /// Scrape a search result and add content
    pub async fn scrape_result(
        &self,
        mut result: SearchResult,
        scrape_request: &ScrapeRequest,
    ) -> SearchResult {
        info!("Scraping search result: {}", result.url);

        // Create scrape request with the result URL
        let mut req = scrape_request.clone();
        req.url = result.url.clone();

        match self.scrape_url(&req).await {
            Ok(document) => {
                result.content = Some(document);
            }
            Err(e) => {
                warn!("Failed to scrape {}: {}", result.url, e);
                // Continue without content
            }
        }

        result
    }

    /// Internal method to scrape a URL
    async fn scrape_url(&self, request: &ScrapeRequest) -> Result<Document> {
        let engine = HttpEngine::with_options(request.timeout, request.skip_tls_verification)?;
        let raw_result = engine.scrape(request).await?;
        let document = format::process_scrape_result(raw_result, request).await?;
        Ok(document)
    }
}

impl Default for SearchProvider {
    fn default() -> Self {
        Self::new().expect("Failed to create default search provider")
    }
}

/// Extract the actual URL from DuckDuckGo's redirect URL
fn extract_url_from_duckduckgo(url: &str) -> String {
    // DuckDuckGo uses URLs like: //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com
    if url.starts_with("//duckduckgo.com/l/?") {
        // Parse query parameters
        if let Some(query_start) = url.find('?') {
            let query = &url[query_start + 1..];
            for param in query.split('&') {
                if let Some(eq_pos) = param.find('=') {
                    let key = &param[..eq_pos];
                    let value = &param[eq_pos + 1..];
                    if key == "uddg" {
                        return urlencoding::decode(value).unwrap_or_default().to_string();
                    }
                }
            }
        }
    }

    url.to_string()
}

// We need to add urlencoding to Cargo.toml for URL encoding
// For now, let's use a simple implementation

mod urlencoding {
    pub fn encode(s: &str) -> String {
        percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
    }

    pub fn decode(s: &str) -> Result<String, std::str::Utf8Error> {
        percent_encoding::percent_decode_str(s)
            .decode_utf8()
            .map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url_from_duckduckgo() {
        let ddg_url = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpage";
        let result = extract_url_from_duckduckgo(ddg_url);
        assert_eq!(result, "https://example.com/page");
    }

    #[test]
    fn test_extract_url_passthrough() {
        let normal_url = "https://example.com";
        let result = extract_url_from_duckduckgo(normal_url);
        assert_eq!(result, normal_url);
    }
}
