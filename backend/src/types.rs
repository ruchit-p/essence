use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main scrape request matching Firecrawl v1 schema
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeRequest {
    /// Required: URL to scrape
    pub url: String,

    /// Output formats (default: ["markdown"])
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,

    /// Headers to send with request
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// CSS selectors to include
    #[serde(default)]
    pub include_tags: Vec<String>,

    /// CSS selectors to exclude
    #[serde(default)]
    pub exclude_tags: Vec<String>,

    /// Extract only main content (default: true)
    #[serde(default = "default_true")]
    pub only_main_content: bool,

    /// Request timeout in milliseconds (default: 30000)
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Wait time before scraping in milliseconds (default: 0)
    #[serde(default)]
    pub wait_for: u64,

    /// Remove base64 images (default: true)
    #[serde(default = "default_true")]
    pub remove_base64_images: bool,

    /// Skip TLS verification
    #[serde(default)]
    pub skip_tls_verification: bool,

    /// Engine to use: "auto" | "http" | "browser" (default: "auto")
    #[serde(default = "default_engine")]
    pub engine: String,

    /// CSS selector to wait for before scraping (browser only)
    #[serde(default)]
    pub wait_for_selector: Option<String>,

    /// Browser actions to perform before scraping
    #[serde(default)]
    pub actions: Vec<BrowserAction>,

    /// Capture screenshot (browser only)
    #[serde(default)]
    pub screenshot: bool,

    /// Screenshot format: "png" | "jpeg" (default: "png")
    #[serde(default = "default_screenshot_format")]
    pub screenshot_format: String,
}

/// Browser actions to perform
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrowserAction {
    Click { selector: String },
    Type { selector: String, text: String },
    Scroll { direction: String },
    Wait { milliseconds: u64 },
    WaitForSelector { selector: String },
}

// Default functions for ScrapeRequest
fn default_formats() -> Vec<String> {
    vec!["markdown".to_string()]
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30000
}

fn default_engine() -> String {
    "auto".to_string()
}

fn default_screenshot_format() -> String {
    "png".to_string()
}

impl Default for ScrapeRequest {
    fn default() -> Self {
        Self {
            url: String::new(),
            formats: default_formats(),
            headers: HashMap::new(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            only_main_content: default_true(),
            timeout: default_timeout(),
            wait_for: 0,
            remove_base64_images: default_true(),
            skip_tls_verification: false,
            engine: default_engine(),
            wait_for_selector: None,
            actions: Vec::new(),
            screenshot: false,
            screenshot_format: default_screenshot_format(),
        }
    }
}

/// Scrape response matching Firecrawl v1 schema
#[derive(Debug, Clone, Serialize)]
pub struct ScrapeResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Document>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrape_id: Option<String>,
}

/// Document structure containing scraped data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Page title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Page description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Page URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Markdown content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,

    /// HTML content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,

    /// Raw HTML
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_html: Option<String>,

    /// Links found on page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<String>>,

    /// Images found on page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,

    /// Screenshot (base64 encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,

    /// Metadata
    pub metadata: Metadata,
}

/// Metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub robots: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub og_title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub og_description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub og_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub og_image: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,

    pub status_code: u16,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,

    // Advanced extraction metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_count: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_time: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,

    // Engine detection metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_frameworks: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_reason: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_script_ratio: Option<f64>,
}

// Default implementations
impl Default for Metadata {
    fn default() -> Self {
        Self {
            title: None,
            description: None,
            language: None,
            keywords: None,
            robots: None,
            og_title: None,
            og_description: None,
            og_url: None,
            og_image: None,
            url: None,
            source_url: None,
            status_code: 200,
            content_type: None,
            canonical_url: None,
            word_count: None,
            reading_time: None,
            excerpt: None,
            detected_frameworks: None,
            detection_reason: None,
            content_script_ratio: None,
        }
    }
}


// Default function for optional bools
fn default_true_option() -> Option<bool> {
    Some(true)
}

impl ScrapeResponse {
    pub fn success(data: Document) -> Self {
        Self {
            success: true,
            warning: None,
            data: Some(data),
            error: None,
            scrape_id: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            warning: None,
            data: None,
            error: Some(error),
            scrape_id: None,
        }
    }
}

/// Map request matching Firecrawl v1 schema
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRequest {
    /// Required: URL to map
    pub url: String,

    /// Search query to filter URLs
    #[serde(default)]
    pub search: Option<String>,

    /// Skip sitemap.xml (default: false)
    #[serde(default)]
    pub ignore_sitemap: Option<bool>,

    /// Include subdomains (default: true)
    #[serde(default = "default_include_subdomains")]
    pub include_subdomains: Option<bool>,

    /// Max URLs to return (default: 5000, max: 100000)
    #[serde(default = "default_map_limit")]
    pub limit: Option<u32>,
}

/// Map response matching Firecrawl v1 schema
#[derive(Debug, Clone, Serialize)]
pub struct MapResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrape_id: Option<String>,
}

fn default_include_subdomains() -> Option<bool> {
    Some(true)
}

fn default_map_limit() -> Option<u32> {
    Some(5000)
}

impl MapResponse {
    pub fn success(links: Vec<String>) -> Self {
        Self {
            success: true,
            links: Some(links),
            error: None,
            scrape_id: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            links: None,
            error: Some(error),
            scrape_id: None,
        }
    }
}

/// Crawl request matching Firecrawl v1 crawl schema
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlRequest {
    /// Required: Starting URL
    pub url: String,

    /// Patterns to exclude (glob patterns)
    #[serde(default)]
    pub exclude_paths: Option<Vec<String>>,

    /// Patterns to include (glob patterns)
    #[serde(default)]
    pub include_paths: Option<Vec<String>>,

    /// Max crawl depth (default: 2)
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Max pages to crawl (default: 100)
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Allow backward links (crawl entire domain)
    #[serde(default)]
    pub allow_backward_links: Option<bool>,

    /// Allow external links
    #[serde(default)]
    pub allow_external_links: Option<bool>,

    /// Ignore sitemap
    #[serde(default)]
    pub ignore_sitemap: Option<bool>,

    /// Enable pagination detection (default: true)
    #[serde(default = "default_true_option")]
    pub detect_pagination: Option<bool>,

    /// Maximum pagination pages to follow (default: 50)
    #[serde(default = "default_max_pagination_pages")]
    pub max_pagination_pages: Option<u32>,

    /// Use parallel crawler for better performance (default: false)
    #[serde(default)]
    pub use_parallel: Option<bool>,
}

/// Crawl response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<Document>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Crawl ID for three-phase crawls (poll for status)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crawl_id: Option<String>,
    /// Status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

fn default_max_depth() -> u32 {
    2
}

fn default_limit() -> u32 {
    100
}

fn default_max_pagination_pages() -> Option<u32> {
    Some(50)
}

impl CrawlResponse {
    pub fn success(data: Vec<Document>) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            crawl_id: None,
            message: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            crawl_id: None,
            message: None,
        }
    }

    pub fn started(crawl_id: String) -> Self {
        Self {
            success: true,
            data: None,
            error: None,
            crawl_id: Some(crawl_id.clone()),
            message: Some(format!("Crawl started with ID: {}", crawl_id)),
        }
    }
}

// ===== Search Types =====

/// Search request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// Search query
    pub query: String,

    /// Max results to return (default: 10)
    #[serde(default = "default_search_limit")]
    pub limit: u32,

    /// Whether to scrape each result URL (default: false)
    #[serde(default)]
    pub scrape_results: bool,

    /// Scrape options to apply if scraping results
    #[serde(default)]
    pub scrape_options: Option<ScrapeOptions>,
}

/// Scrape options for search results
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeOptions {
    /// Formats to return (default: ["markdown"])
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,

    /// Extract only main content (default: true)
    #[serde(default = "default_true")]
    pub only_main_content: bool,

    /// Timeout in milliseconds (default: 10000)
    #[serde(default = "default_scrape_timeout")]
    pub timeout: u64,
}

/// Search response
#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<SearchResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Individual search result
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// Title of the search result
    pub title: String,
    /// URL of the search result
    pub url: String,
    /// Snippet/description from search engine
    pub snippet: String,
    /// Scraped content (if scrape_results was true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Document>,
}

fn default_search_limit() -> u32 {
    10
}

fn default_scrape_timeout() -> u64 {
    10000
}

impl SearchResponse {
    pub fn success(data: Vec<SearchResult>) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

// ===== Extract Types =====

/// Extract request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractRequest {
    /// URLs to scrape and extract from
    pub urls: Vec<String>,

    /// Prompt to guide extraction
    pub prompt: String,

    /// JSON Schema to validate against (optional)
    #[serde(default)]
    pub schema: Option<serde_json::Value>,

    /// LLM model to use (default: claude-3-5-sonnet-20241022)
    #[serde(default = "default_llm_model")]
    pub model: String,

    /// Scrape options
    #[serde(default)]
    pub scrape_options: Option<ScrapeOptions>,
}

/// LLM usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LLMUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub estimated_cost_usd: f64,
}

/// Extract response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_usage: Option<LLMUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<HashMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_urls_scraped: Option<usize>,
}

fn default_llm_model() -> String {
    std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string())
}

impl ExtractResponse {
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            validation_errors: None,
            warnings: None,
            extract_id: None,
            llm_usage: None,
            sources: None,
            total_urls_scraped: None,
        }
    }

    pub fn success_with_metadata(
        data: serde_json::Value,
        extract_id: String,
        llm_usage: Option<LLMUsage>,
        sources: Option<HashMap<String, Vec<String>>>,
        total_urls_scraped: Option<usize>,
    ) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            validation_errors: None,
            warnings: None,
            extract_id: Some(extract_id),
            llm_usage,
            sources,
            total_urls_scraped,
        }
    }

    pub fn success_with_warnings(
        data: serde_json::Value,
        extract_id: String,
        llm_usage: Option<LLMUsage>,
        sources: Option<HashMap<String, Vec<String>>>,
        total_urls_scraped: Option<usize>,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            validation_errors: None,
            warnings: if warnings.is_empty() {
                None
            } else {
                Some(warnings)
            },
            extract_id: Some(extract_id),
            llm_usage,
            sources,
            total_urls_scraped,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            validation_errors: None,
            warnings: None,
            extract_id: None,
            llm_usage: None,
            sources: None,
            total_urls_scraped: None,
        }
    }

    pub fn validation_error(error: String, errors: Vec<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            validation_errors: Some(errors),
            warnings: None,
            extract_id: None,
            llm_usage: None,
            sources: None,
            total_urls_scraped: None,
        }
    }

    pub fn validation_error_with_metadata(
        error: String,
        errors: Vec<String>,
        extract_id: String,
        llm_usage: Option<LLMUsage>,
        total_urls_scraped: Option<usize>,
    ) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            validation_errors: Some(errors),
            warnings: None,
            extract_id: Some(extract_id),
            llm_usage,
            sources: None,
            total_urls_scraped,
        }
    }
}

// ===== Streaming Crawl Types =====

/// Crawl event types for SSE streaming
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CrawlEvent {
    /// Crawl started event
    Status {
        pages_crawled: usize,
        queue_size: usize,
        current_url: Option<String>,
    },
    /// Document completed event
    Document {
        url: String,
        title: Option<String>,
        markdown: Option<String>,
        metadata: Box<Metadata>,
    },
    /// Error event for individual URL
    Error {
        url: String,
        error: String,
    },
    /// Crawl completion event
    Complete {
        total_pages: usize,
        success: usize,
        errors: usize,
    },
}
