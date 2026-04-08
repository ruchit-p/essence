//! MCP (Model Context Protocol) server implementation for Essence.
//!
//! Exposes Essence's web retrieval capabilities as MCP tools so that AI agents
//! (Claude, etc.) can use scrape, map, crawl, and search functionality.

use rmcp::{
    handler::server::tool::ToolRouter, handler::server::wrapper::Parameters, model::*, tool,
    tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::{error, info};

use crate::{
    api::{llmstxt::llmstxt_core_logic, scrape::scrape_core_logic},
    crawler::{crawl_website, mapper},
    search::SearchProvider,
    types::{CrawlRequest, LlmsTxtRequest, MapRequest, ScrapeRequest},
};

// ---------------------------------------------------------------------------
// Parameter structs (derive JsonSchema for MCP tool input schema generation)
// ---------------------------------------------------------------------------

/// Parameters for the `scrape` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ScrapeParams {
    /// The URL to scrape.
    pub url: String,

    /// Output formats to return (e.g. "markdown", "html", "links").
    /// Defaults to ["markdown"].
    #[serde(default)]
    pub formats: Option<Vec<String>>,

    /// Rendering engine: "auto", "http", or "browser".
    /// Defaults to "auto".
    #[serde(default)]
    pub engine: Option<String>,

    /// Request timeout in milliseconds. Defaults to 30000.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Parameters for the `map` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MapParams {
    /// The URL to discover links from.
    pub url: String,

    /// Search query to filter discovered URLs.
    #[serde(default)]
    pub search: Option<String>,

    /// Skip sitemap.xml discovery.
    #[serde(default)]
    pub ignore_sitemap: Option<bool>,

    /// Include subdomains in discovery.
    #[serde(default)]
    pub include_subdomains: Option<bool>,

    /// Maximum number of URLs to return. Defaults to 5000.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for the `crawl` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CrawlParams {
    /// The starting URL to crawl.
    pub url: String,

    /// Maximum crawl depth. Defaults to 2.
    #[serde(default)]
    pub max_depth: Option<u32>,

    /// Maximum number of pages to crawl. Defaults to 100.
    #[serde(default)]
    pub limit: Option<u32>,

    /// Glob patterns of paths to include.
    #[serde(default)]
    pub include_paths: Option<Vec<String>>,

    /// Glob patterns of paths to exclude.
    #[serde(default)]
    pub exclude_paths: Option<Vec<String>>,

    /// Allow following links back up the URL hierarchy.
    #[serde(default)]
    pub allow_backward_links: Option<bool>,

    /// Allow following external links.
    #[serde(default)]
    pub allow_external_links: Option<bool>,

    /// Rendering engine: "auto", "http", or "browser". Defaults to "http".
    /// "auto" tries HTTP first, falls back to browser for JS-heavy pages.
    #[serde(default)]
    pub engine: Option<String>,
}

/// Parameters for the `search` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// The search query.
    pub query: String,

    /// Number of results to return. Defaults to 10.
    #[serde(default)]
    pub limit: Option<u32>,

    /// Whether to scrape the content of each result URL. Defaults to false.
    #[serde(default)]
    pub scrape_results: Option<bool>,
}

/// Parameters for the `llmstxt` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LlmsTxtParams {
    /// The website URL to generate llms.txt for.
    pub url: String,

    /// Maximum number of URLs to process. Defaults to 20.
    #[serde(default)]
    pub max_urls: Option<u32>,

    /// OpenAI-compatible LLM API base URL for generating descriptions.
    /// If not provided, page metadata descriptions are used.
    #[serde(default)]
    pub llm_base_url: Option<String>,

    /// LLM model name (e.g. "gpt-4o-mini"). Only used if llm_base_url is set.
    #[serde(default)]
    pub llm_model: Option<String>,

    /// API key for the LLM service. Only used if llm_base_url is set.
    #[serde(default)]
    pub llm_api_key: Option<String>,

    /// Whether to include full page content (llms-full.txt). Defaults to true.
    #[serde(default)]
    pub show_full_text: Option<bool>,

    /// Rendering engine: "auto", "http", or "browser". Defaults to "auto".
    /// "auto" tries HTTP first, falls back to browser for JS-heavy pages.
    #[serde(default)]
    pub engine: Option<String>,
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// The Essence MCP server, exposing scrape/map/crawl/search as MCP tools.
#[derive(Clone)]
pub struct EssenceMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl EssenceMcpServer {
    /// Create a new `EssenceMcpServer` with all tool routes registered.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Scrape a single web page and return its content as Markdown, HTML, or other formats.
    ///
    /// Uses an intelligent HTTP -> Browser fallback strategy for maximum reliability.
    #[tool(
        description = "Scrape a single web page and return its content as Markdown (default), HTML, or other formats. Uses intelligent HTTP -> Browser fallback for reliability."
    )]
    async fn scrape(
        &self,
        Parameters(params): Parameters<ScrapeParams>,
    ) -> Result<CallToolResult, McpError> {
        info!("MCP tool call: scrape url={}", params.url);

        let request = ScrapeRequest {
            url: params.url.clone(),
            formats: params
                .formats
                .unwrap_or_else(|| vec!["markdown".to_string()]),
            engine: params.engine.unwrap_or_else(|| "auto".to_string()),
            timeout: params.timeout_ms.unwrap_or(30000),
            ..ScrapeRequest::default()
        };

        match scrape_core_logic(&request).await {
            Ok(response) => {
                let json = serde_json::to_string_pretty(&response).map_err(|e| {
                    McpError::internal_error(
                        format!("Failed to serialize scrape response: {}", e),
                        None,
                    )
                })?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                error!("MCP scrape error for {}: {}", params.url, e);
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "Scrape failed: {}",
                    e
                ))]))
            }
        }
    }

    /// Discover URLs from a website via sitemaps and in-page link extraction.
    #[tool(
        description = "Discover URLs from a website via sitemaps and in-page link extraction. Returns a list of discovered URLs."
    )]
    async fn map(
        &self,
        Parameters(params): Parameters<MapParams>,
    ) -> Result<CallToolResult, McpError> {
        info!("MCP tool call: map url={}", params.url);

        let map_request = MapRequest {
            url: params.url.clone(),
            search: params.search,
            ignore_sitemap: params.ignore_sitemap,
            include_subdomains: params.include_subdomains.or(Some(true)),
            limit: params.limit.or(Some(5000)),
        };

        match mapper::discover_urls(&params.url, &map_request).await {
            Ok(links) => {
                let result = serde_json::json!({
                    "success": true,
                    "count": links.len(),
                    "links": links
                });
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::internal_error(
                        format!("Failed to serialize map response: {}", e),
                        None,
                    )
                })?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                error!("MCP map error for {}: {}", params.url, e);
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "Map failed: {}",
                    e
                ))]))
            }
        }
    }

    /// Crawl a website starting from a URL, following links up to a specified depth and page limit.
    #[tool(
        description = "Crawl a website starting from a URL, following links up to a specified depth and page limit. Returns scraped content from all crawled pages."
    )]
    async fn crawl(
        &self,
        Parameters(params): Parameters<CrawlParams>,
    ) -> Result<CallToolResult, McpError> {
        info!("MCP tool call: crawl url={}", params.url);

        let crawl_request = CrawlRequest {
            url: params.url.clone(),
            max_depth: params.max_depth.unwrap_or(2),
            limit: params.limit.unwrap_or(100),
            include_paths: params.include_paths,
            exclude_paths: params.exclude_paths,
            allow_backward_links: params.allow_backward_links,
            allow_external_links: params.allow_external_links,
            ignore_sitemap: None,
            detect_pagination: Some(true),
            max_pagination_pages: Some(50),
            use_parallel: None,
            engine: params.engine,
        };

        match crawl_website(&crawl_request).await {
            Ok(documents) => {
                let result = serde_json::json!({
                    "success": true,
                    "pages_crawled": documents.len(),
                    "data": documents
                });
                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::internal_error(
                        format!("Failed to serialize crawl response: {}", e),
                        None,
                    )
                })?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                error!("MCP crawl error for {}: {}", params.url, e);
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "Crawl failed: {}",
                    e
                ))]))
            }
        }
    }

    /// Search the web using DuckDuckGo and optionally scrape each result page.
    #[tool(
        description = "Search the web using DuckDuckGo and optionally scrape each result page for full content. Returns search results with titles, URLs, and snippets."
    )]
    async fn search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        info!("MCP tool call: search query={}", params.query);

        let provider = SearchProvider::new().map_err(|e| {
            McpError::internal_error(format!("Failed to create search provider: {}", e), None)
        })?;

        let limit = params.limit.unwrap_or(10);

        let mut results = provider
            .search_duckduckgo(&params.query, limit)
            .await
            .map_err(|e| McpError::internal_error(format!("Search failed: {}", e), None))?;

        // Optionally scrape each result
        if params.scrape_results.unwrap_or(false) {
            info!("Scraping {} search results", results.len());
            for result in &mut results {
                let scrape_req = ScrapeRequest {
                    url: result.url.clone(),
                    formats: vec!["markdown".to_string()],
                    engine: "http".to_string(),
                    timeout: 10000,
                    only_main_content: true,
                    ..ScrapeRequest::default()
                };
                match scrape_core_logic(&scrape_req).await {
                    Ok(response) => {
                        if let Some(data) = response.data {
                            result.content = Some(data);
                        }
                    }
                    Err(e) => {
                        error!("Failed to scrape search result {}: {}", result.url, e);
                    }
                }
            }
        }

        let response = serde_json::json!({
            "success": true,
            "count": results.len(),
            "data": results
        });
        let json = serde_json::to_string_pretty(&response).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize search response: {}", e), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Generate llms.txt and llms-full.txt files from a documentation website.
    ///
    /// Discovers URLs, scrapes each page, and builds structured text files
    /// optimized for LLM consumption.
    #[tool(
        description = "Generate llms.txt and llms-full.txt from a website. Discovers URLs via sitemaps/links, scrapes each page, and produces structured index + full-text files for LLM consumption. Optionally uses an OpenAI-compatible API to generate concise page descriptions."
    )]
    async fn llmstxt(
        &self,
        Parameters(params): Parameters<LlmsTxtParams>,
    ) -> Result<CallToolResult, McpError> {
        info!("MCP tool call: llmstxt url={}", params.url);

        let request = LlmsTxtRequest {
            url: params.url.clone(),
            max_urls: params.max_urls.unwrap_or(20),
            llm_base_url: params.llm_base_url,
            llm_model: params.llm_model,
            llm_api_key: params.llm_api_key,
            max_concurrent_scrapes: 10,
            show_full_text: params.show_full_text.unwrap_or(true),
            ignore_sitemap: None,
            include_subdomains: Some(true),
            engine: params.engine.unwrap_or_else(|| "auto".to_string()),
        };

        match llmstxt_core_logic(&request).await {
            Ok(response) => {
                let json = serde_json::to_string_pretty(&response).map_err(|e| {
                    McpError::internal_error(
                        format!("Failed to serialize llmstxt response: {}", e),
                        None,
                    )
                })?;
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => {
                error!("MCP llmstxt error for {}: {}", params.url, e);
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "llms.txt generation failed: {}",
                    e
                ))]))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ServerHandler implementation
// ---------------------------------------------------------------------------

#[tool_handler]
impl ServerHandler for EssenceMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "essence".to_string(),
                title: Some("Essence Web Retrieval Engine".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                description: Some(
                    "Production-ready web retrieval engine with intelligent HTTP->Browser fallback, \
                     providing LLM-ready Markdown outputs. Supports scraping, crawling, URL discovery, \
                     and web search."
                        .to_string(),
                ),
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            instructions: Some(
                "Essence is a web retrieval engine. Use the 'scrape' tool to fetch a single page, \
                 'map' to discover URLs on a site, 'crawl' to traverse multiple pages, 'search' \
                 to find pages via DuckDuckGo web search, or 'llmstxt' to generate llms.txt and \
                 llms-full.txt files from a documentation website. All tools return structured JSON \
                 with Markdown content suitable for LLM consumption."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}
