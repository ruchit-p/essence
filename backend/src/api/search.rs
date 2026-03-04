use crate::{
    error::ScrapeError,
    search::SearchProvider,
    types::{ScrapeOptions, ScrapeRequest, SearchRequest, SearchResponse},
    validation,
};
use axum::Json;
use futures::{stream, StreamExt};
use std::env;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Handler for POST /api/v1/search
pub async fn search_handler(
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ScrapeError> {
    info!("Search request received for query: {}", request.query);

    // Validate request
    validation::validate_search_request(&request)?;

    // Create search provider
    let provider = SearchProvider::new().map_err(|e| {
        error!("Failed to create search provider: {}", e);
        e
    })?;

    // Perform search
    let mut results = provider
        .search_duckduckgo(&request.query, request.limit)
        .await
        .map_err(|e| {
            error!("Search failed: {}", e);
            e
        })?;

    info!("Found {} search results", results.len());

    // Optionally scrape each result
    if request.scrape_results {
        info!("Scraping {} search results in parallel", results.len());

        // Get max parallel scrapes from environment (default: 5)
        let max_parallel = env::var("MAX_PARALLEL_SCRAPES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(5);

        info!("Using max_parallel_scrapes = {}", max_parallel);

        // Build scrape request from options
        let scrape_options = request.scrape_options.as_ref();

        // Create a single shared provider (more efficient than creating one per result)
        let provider = Arc::new(provider);

        // Scrape results in parallel with chunked buffering
        let start_time = std::time::Instant::now();

        results = stream::iter(results)
            .map(|result| {
                let scrape_req = build_scrape_request(&result.url, scrape_options);
                let provider = Arc::clone(&provider);
                async move { provider.scrape_result(result, &scrape_req).await }
            })
            .buffer_unordered(max_parallel) // Process max_parallel requests concurrently
            .collect::<Vec<_>>()
            .await;

        let elapsed = start_time.elapsed();
        let success_count = results.iter().filter(|r| r.content.is_some()).count();
        let failure_count = results.len() - success_count;

        info!(
            "Scraping complete: {} successful, {} failed in {:.2}s ({:.2}s avg per result)",
            success_count,
            failure_count,
            elapsed.as_secs_f64(),
            elapsed.as_secs_f64() / results.len() as f64
        );

        if failure_count > 0 {
            warn!(
                "{} of {} scrapes failed (returning partial results)",
                failure_count,
                results.len()
            );
        }
    }

    Ok(Json(SearchResponse::success(results)))
}

/// Build a ScrapeRequest from URL and options
fn build_scrape_request(url: &str, options: Option<&ScrapeOptions>) -> ScrapeRequest {
    let opts = options.cloned().unwrap_or_else(|| ScrapeOptions {
        formats: vec!["markdown".to_string()],
        only_main_content: true,
        timeout: 10000,
    });

    ScrapeRequest {
        url: url.to_string(),
        formats: opts.formats,
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: opts.only_main_content,
        timeout: opts.timeout,
        wait_for: 0,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "http".to_string(), // Use HTTP for speed
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_scrape_request_default() {
        let req = build_scrape_request("https://example.com", None);
        assert_eq!(req.url, "https://example.com");
        assert_eq!(req.formats, vec!["markdown"]);
        assert_eq!(req.timeout, 10000);
        assert!(req.only_main_content);
    }

    #[test]
    fn test_build_scrape_request_custom() {
        let options = ScrapeOptions {
            formats: vec!["html".to_string(), "markdown".to_string()],
            only_main_content: false,
            timeout: 5000,
        };
        let req = build_scrape_request("https://example.com", Some(&options));
        assert_eq!(req.formats, vec!["html", "markdown"]);
        assert_eq!(req.timeout, 5000);
        assert!(!req.only_main_content);
    }

    #[tokio::test]
    async fn test_search_handler_empty_query() {
        let request = SearchRequest {
            query: "".to_string(),
            limit: 10,
            scrape_results: false,
            scrape_options: None,
        };

        let result = search_handler(Json(request)).await;
        assert!(result.is_err());
    }
}
