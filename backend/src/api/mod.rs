pub mod crawl;
pub mod extract;
pub mod llmstxt;
pub mod map;
pub mod scrape;
pub mod search;

use axum::{
    routing::{get, post},
    Json, Router,
};

use crate::types::*;

/// OpenAPI documentation for the Essence API
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "Essence Web Retrieval Engine",
        description = "Production-ready web retrieval engine with intelligent HTTP→Browser fallback, providing LLM-ready Markdown output.",
        version = env!("CARGO_PKG_VERSION"),
        license(name = "MIT"),
    ),
    paths(
        scrape::scrape_handler,
        map::map_handler,
        crawl::crawl_handler,
        search::search_handler,
        extract::extract_handler,
        llmstxt::llmstxt_handler,
    ),
    components(schemas(
        ScrapeRequest, ScrapeResponse, Document, Metadata, BrowserAction,
        MapRequest, MapResponse,
        CrawlRequest, CrawlResponse,
        SearchRequest, SearchResponse, SearchResult, ScrapeOptions,
        ExtractRequest, ExtractResponse,
        LlmsTxtRequest, LlmsTxtResponse,
    ))
)]
pub struct ApiDoc;

/// Create the API router with core endpoints
pub fn create_router() -> Router {
    Router::new()
        .route("/api/v1/scrape", post(scrape::scrape_handler))
        .route("/api/v1/map", post(map::map_handler))
        .route("/api/v1/crawl", post(crawl::crawl_handler))
        .route("/api/v1/crawl/stream", post(crawl::crawl_stream_handler))
        .route("/api/v1/search", post(search::search_handler))
        .route("/api/v1/extract", post(extract::extract_handler))
        .route("/api/v1/llmstxt", post(llmstxt::llmstxt_handler))
        .route("/api/docs/openapi.json", get(openapi_spec))
        .route("/health", get(health_handler))
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    use utoipa::OpenApi;
    Json(ApiDoc::openapi())
}
