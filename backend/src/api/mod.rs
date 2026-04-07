pub mod crawl;
pub mod llmstxt;
pub mod map;
pub mod scrape;
pub mod search;

use axum::{routing::{get, post}, Router};

/// Create the API router with core endpoints
pub fn create_router() -> Router {
    Router::new()
        .route("/api/v1/scrape", post(scrape::scrape_handler))
        .route("/api/v1/map", post(map::map_handler))
        .route("/api/v1/crawl", post(crawl::crawl_handler))
        .route("/api/v1/crawl/stream", post(crawl::crawl_stream_handler))
        .route("/api/v1/search", post(search::search_handler))
        .route("/api/v1/llmstxt", post(llmstxt::llmstxt_handler))
        .route("/health", get(health_handler))
}

async fn health_handler() -> &'static str {
    "ok"
}
