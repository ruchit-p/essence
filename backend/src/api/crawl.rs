use crate::{
    crawler::{crawl_website, crawl_website_stream, ParallelCrawler},
    error::ScrapeError,
    types::{CrawlEvent, CrawlRequest, CrawlResponse},
    validation,
};
use axum::{
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::{Stream, StreamExt};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info, warn};

/// Handler for POST /api/v1/crawl
#[utoipa::path(
    post,
    path = "/api/v1/crawl",
    request_body = CrawlRequest,
    responses(
        (status = 200, description = "Crawl completed successfully", body = CrawlResponse),
        (status = 400, description = "Invalid request"),
    ),
    tag = "Crawl"
)]
pub async fn crawl_handler(
    Json(request): Json<CrawlRequest>,
) -> Result<Json<CrawlResponse>, ScrapeError> {
    info!("Crawl request received for URL: {}", request.url);

    // Validate request (includes SSRF protection)
    validation::validate_crawl_request(&request).await?;

    // Log crawl parameters
    info!(
        "Crawl parameters - max_depth: {}, limit: {}, allow_backward_links: {:?}, allow_external_links: {:?}",
        request.max_depth,
        request.limit,
        request.allow_backward_links,
        request.allow_external_links
    );

    // Execute the crawl with timeout
    let crawl_timeout = validation::get_crawl_timeout();
    let use_parallel = request.use_parallel.unwrap_or(false);

    let result = if use_parallel {
        info!("Using parallel crawler for better performance");
        let parallel_crawler = ParallelCrawler::new();
        timeout(crawl_timeout, parallel_crawler.crawl_parallel(&request))
            .await
            .map_err(|_| {
                warn!("Parallel crawl timeout after {:?}", crawl_timeout);
                ScrapeError::Timeout
            })?
            .map_err(|e| {
                error!("Failed to crawl website {} (parallel): {}", request.url, e);
                e
            })
    } else {
        timeout(crawl_timeout, crawl_website(&request))
            .await
            .map_err(|_| {
                warn!("Crawl timeout after {:?}", crawl_timeout);
                ScrapeError::Timeout
            })?
            .map_err(|e| {
                error!("Failed to crawl website {}: {}", request.url, e);
                e
            })
    };

    let documents = result?;

    info!(
        "Crawl completed for URL: {} - {} pages scraped",
        request.url,
        documents.len()
    );

    Ok(Json(CrawlResponse::success(documents)))
}

/// Handler for POST /api/v1/crawl/stream - SSE streaming endpoint
///
/// Streams crawl events as Server-Sent Events (SSE):
/// - status: Crawl progress updates
/// - document: Completed documents
/// - error: Error events for individual URLs
/// - complete: Final summary
///
/// Example usage with curl:
/// ```bash
/// curl -N -X POST http://localhost:8080/api/v1/crawl/stream \
///   -H "Content-Type: application/json" \
///   -d '{"url": "https://example.com", "limit": 50, "max_depth": 2}'
/// ```
pub async fn crawl_stream_handler(
    Json(request): Json<CrawlRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ScrapeError> {
    info!("Streaming crawl request received for URL: {}", request.url);

    // Validate request (includes SSRF protection)
    validation::validate_crawl_request(&request).await?;

    // Log crawl parameters
    info!(
        "Crawl parameters - max_depth: {}, limit: {}, allow_backward_links: {:?}, allow_external_links: {:?}",
        request.max_depth,
        request.limit,
        request.allow_backward_links,
        request.allow_external_links
    );

    // Create channel for streaming events
    // Buffer size of 100 allows crawler to continue working even if client is slow
    let (tx, rx) = mpsc::channel::<crate::error::Result<CrawlEvent>>(100);

    // Spawn crawler task in background
    tokio::spawn(async move {
        let result = crawl_website_stream(request, tx).await;

        if let Err(e) = result {
            error!("Streaming crawl failed: {}", e);
        }
    });

    // Convert receiver to SSE stream
    let stream = ReceiverStream::new(rx).map(|event_result| {
        match event_result {
            Ok(crawl_event) => {
                // Serialize event to JSON
                match serde_json::to_string(&crawl_event) {
                    Ok(json) => {
                        // Determine event name based on type
                        let event_name = match &crawl_event {
                            CrawlEvent::Status { .. } => "status",
                            CrawlEvent::Document { .. } => "document",
                            CrawlEvent::Error { .. } => "error",
                            CrawlEvent::Complete { .. } => "complete",
                        };

                        Ok(Event::default()
                            .event(event_name)
                            .data(json))
                    }
                    Err(e) => {
                        error!("Failed to serialize crawl event: {}", e);
                        Ok(Event::default()
                            .event("error")
                            .data(format!(r#"{{"type":"error","url":"","error":"Failed to serialize event: {}"}}"#, e)))
                    }
                }
            }
            Err(e) => {
                // Send error event
                Ok(Event::default()
                    .event("error")
                    .data(format!(r#"{{"type":"error","url":"","error":"{}"}}"#, e)))
            }
        }
    });

    // Create SSE response with keep-alive
    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crawl_handler_invalid_url() {
        let request = CrawlRequest {
            url: "".to_string(),
            exclude_paths: None,
            include_paths: None,
            max_depth: 2,
            limit: 100,
            allow_backward_links: None,
            allow_external_links: None,
            ignore_sitemap: None,
            detect_pagination: None,
            max_pagination_pages: None,
            use_parallel: None,
            engine: None,
        };

        let result = crawl_handler(Json(request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_crawl_stream_handler_invalid_url() {
        let request = CrawlRequest {
            url: "".to_string(),
            exclude_paths: None,
            include_paths: None,
            max_depth: 2,
            limit: 100,
            allow_backward_links: None,
            allow_external_links: None,
            ignore_sitemap: None,
            detect_pagination: None,
            max_pagination_pages: None,
            use_parallel: None,
            engine: None,
        };

        let result = crawl_stream_handler(Json(request)).await;
        assert!(result.is_err());
    }
}
