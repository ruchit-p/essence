use crate::{
    api::AppState,
    api::scrape::scrape_core_logic,
    crawler::rate_limiter::DomainRateLimiter,
    error::ScrapeError,
    jobs::store::{BatchScrapeItem, JobProgress, JobResult, JobStatus, JobType},
    types::ScrapeRequest,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info};
use url::Url;

// ---------------------------------------------------------------------------
// Request type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchScrapeRequest {
    pub urls: Vec<String>,

    #[serde(default = "default_formats")]
    pub formats: Vec<String>,

    #[serde(default = "default_true")]
    pub only_main_content: bool,

    #[serde(default = "default_timeout")]
    pub timeout: u64,

    #[serde(default)]
    pub headers: HashMap<String, String>,

    #[serde(default = "default_engine")]
    pub engine: String,

    #[serde(default)]
    pub webhook_url: Option<String>,
}

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

// ---------------------------------------------------------------------------
// Core batch logic
// ---------------------------------------------------------------------------

async fn execute_batch_scrape(
    urls: Vec<String>,
    request: &BatchScrapeRequest,
    job_store: Option<(&crate::jobs::store::JobStore, &str)>,
) -> Vec<BatchScrapeItem> {
    let max_concurrency: usize = std::env::var("MAX_BATCH_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let semaphore = Arc::new(Semaphore::new(max_concurrency));
    let rate_limiter = Arc::new(DomainRateLimiter::new(2));

    // Dedup URLs preserving order
    let mut seen = HashSet::new();
    let unique_urls: Vec<String> = urls
        .into_iter()
        .filter(|url| seen.insert(url.clone()))
        .collect();

    let total = unique_urls.len();

    // Spawn all scrape tasks
    let mut handles = Vec::with_capacity(total);

    for url in unique_urls {
        let sem = semaphore.clone();
        let rl = rate_limiter.clone();
        let formats = request.formats.clone();
        let only_main = request.only_main_content;
        let timeout_ms = request.timeout;
        let headers = request.headers.clone();
        let engine_type = request.engine.clone();

        let handle = tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    return BatchScrapeItem {
                        url,
                        success: false,
                        data: None,
                        error: Some("Semaphore closed".to_string()),
                    };
                }
            };

            // Rate limit per domain
            if let Err(e) = rl.wait_for_permission(&url).await {
                return BatchScrapeItem {
                    url,
                    success: false,
                    data: None,
                    error: Some(format!("Rate limiter error: {}", e)),
                };
            }

            // Build ScrapeRequest
            let scrape_req = ScrapeRequest {
                url: url.clone(),
                formats,
                headers,
                only_main_content: only_main,
                timeout: timeout_ms,
                engine: engine_type,
                ..ScrapeRequest::default()
            };

            // Execute scrape with timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                scrape_core_logic(&scrape_req),
            )
            .await
            {
                Ok(Ok(response)) => BatchScrapeItem {
                    url,
                    success: true,
                    data: response.data,
                    error: None,
                },
                Ok(Err(e)) => BatchScrapeItem {
                    url,
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                },
                Err(_) => BatchScrapeItem {
                    url,
                    success: false,
                    data: None,
                    error: Some(format!("Timeout after {}ms", timeout_ms)),
                },
            }
        });

        handles.push(handle);
    }

    // Collect results, updating progress for async mode
    let mut results = Vec::with_capacity(total);
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(item) => results.push(item),
            Err(e) => results.push(BatchScrapeItem {
                url: format!("unknown-{}", i),
                success: false,
                data: None,
                error: Some(format!("Task panicked: {}", e)),
            }),
        }

        // Update progress for async mode
        if let Some((store, job_id)) = &job_store {
            let _ = store.update_progress(
                job_id,
                JobProgress {
                    completed: results.len(),
                    total: Some(total),
                    current_url: None,
                    percent: Some(((results.len() * 100) / total.max(1)) as u8),
                },
            );
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Sync handler: POST /api/v1/batch/scrape
// ---------------------------------------------------------------------------

pub async fn batch_scrape_handler(
    State(_state): State<AppState>,
    Json(request): Json<BatchScrapeRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if request.urls.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "urls array is required and must not be empty"
            })),
        );
    }

    if request.urls.len() > 10 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Maximum 10 URLs for sync batch. Use /api/v1/batch/scrape/async for larger batches."
            })),
        );
    }

    // Validate each URL
    for (i, url) in request.urls.iter().enumerate() {
        if Url::parse(url).is_err() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Invalid URL at index {}: {}", i, url)
                })),
            );
        }
    }

    info!("Batch scrape request for {} URLs", request.urls.len());

    let results = execute_batch_scrape(request.urls.clone(), &request, None).await;

    let success_count = results.iter().filter(|r| r.success).count();
    let error_count = results.len() - success_count;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "data": results,
            "totalUrls": results.len(),
            "successCount": success_count,
            "errorCount": error_count,
        })),
    )
}

// ---------------------------------------------------------------------------
// Async handler: POST /api/v1/batch/scrape/async
// ---------------------------------------------------------------------------

pub async fn async_batch_scrape_handler(
    State(state): State<AppState>,
    Json(request): Json<BatchScrapeRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    if request.urls.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "urls array is required and must not be empty"
            })),
        );
    }

    if request.urls.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Maximum 100 URLs per batch"
            })),
        );
    }

    // Validate each URL
    for (i, url) in request.urls.iter().enumerate() {
        if Url::parse(url).is_err() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Invalid URL at index {}: {}", i, url)
                })),
            );
        }
    }

    let total_urls = request.urls.len();

    info!("Async batch scrape request for {} URLs", total_urls);

    let request_json = serde_json::to_value(&request).unwrap_or_default();

    let job_id = match state.job_store.create_job(
        JobType::BatchScrape,
        request_json,
        request.webhook_url.clone(),
        None,
        None,
    ) {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "success": false,
                    "error": e
                })),
            );
        }
    };

    let store = state.job_store.clone();
    let id = job_id.clone();
    let urls = request.urls.clone();

    tokio::spawn(async move {
        let _ = store.update_status(&id, JobStatus::Running);

        let results = execute_batch_scrape(urls, &request, Some((&store, &id))).await;

        if let Err(e) = store.set_result(&id, JobResult::BatchScrapeResult(results)) {
            error!("Failed to set batch result for job {}: {}", id, e);
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "success": true,
            "batchId": job_id,
            "status": "queued",
            "statusUrl": format!("/api/v1/batch/scrape/async/{}", job_id),
            "totalUrls": total_urls,
        })),
    )
}

// ---------------------------------------------------------------------------
// Status handler: GET /api/v1/batch/scrape/async/:id
// ---------------------------------------------------------------------------

pub async fn async_batch_status_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ScrapeError> {
    let job = state
        .job_store
        .get_job(&id)
        .ok_or_else(|| ScrapeError::NotFound(format!("Job {} not found or expired", id)))?;

    let response = match &job.status {
        JobStatus::Queued | JobStatus::Running => {
            serde_json::json!({
                "success": true,
                "batchId": job.id,
                "status": job.status,
                "progress": job.progress,
                "createdAt": job.created_at,
                "updatedAt": job.updated_at,
            })
        }
        JobStatus::Completed => {
            let (success_count, error_count, total) = match &job.result {
                Some(JobResult::BatchScrapeResult(items)) => {
                    let s = items.iter().filter(|i| i.success).count();
                    (s, items.len() - s, items.len())
                }
                _ => (0, 0, 0),
            };
            serde_json::json!({
                "success": true,
                "batchId": job.id,
                "status": job.status,
                "progress": job.progress,
                "data": job.result,
                "totalUrls": total,
                "successCount": success_count,
                "errorCount": error_count,
                "createdAt": job.created_at,
                "updatedAt": job.updated_at,
            })
        }
        JobStatus::Failed => {
            serde_json::json!({
                "success": false,
                "batchId": job.id,
                "status": job.status,
                "error": job.error,
                "progress": job.progress,
                "createdAt": job.created_at,
                "updatedAt": job.updated_at,
            })
        }
        JobStatus::Cancelled => {
            serde_json::json!({
                "success": true,
                "batchId": job.id,
                "status": job.status,
                "progress": job.progress,
                "createdAt": job.created_at,
                "updatedAt": job.updated_at,
            })
        }
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::store::{JobStore, JobStoreConfig};

    fn test_state() -> AppState {
        AppState {
            job_store: JobStore::new(JobStoreConfig {
                max_jobs: 10,
                result_ttl_secs: 60,
                cleanup_interval_secs: 300,
            }),
        }
    }

    #[tokio::test]
    async fn test_sync_empty_urls() {
        let state = test_state();
        let request = BatchScrapeRequest {
            urls: vec![],
            formats: default_formats(),
            only_main_content: true,
            timeout: 30000,
            headers: HashMap::new(),
            engine: "auto".to_string(),
            webhook_url: None,
        };

        let (status, body) = batch_scrape_handler(State(state), Json(request)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0["success"], false);
    }

    #[tokio::test]
    async fn test_sync_too_many_urls() {
        let state = test_state();
        let urls: Vec<String> = (0..11)
            .map(|i| format!("https://example.com/page{}", i))
            .collect();
        let request = BatchScrapeRequest {
            urls,
            formats: default_formats(),
            only_main_content: true,
            timeout: 30000,
            headers: HashMap::new(),
            engine: "auto".to_string(),
            webhook_url: None,
        };

        let (status, body) = batch_scrape_handler(State(state), Json(request)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.0["error"].as_str().unwrap().contains("10"));
    }

    #[tokio::test]
    async fn test_sync_invalid_url() {
        let state = test_state();
        let request = BatchScrapeRequest {
            urls: vec![
                "https://example.com".to_string(),
                "not-a-url".to_string(),
            ],
            formats: default_formats(),
            only_main_content: true,
            timeout: 30000,
            headers: HashMap::new(),
            engine: "auto".to_string(),
            webhook_url: None,
        };

        let (status, body) = batch_scrape_handler(State(state), Json(request)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.0["error"].as_str().unwrap().contains("index 1"));
    }

    #[tokio::test]
    async fn test_async_empty_urls() {
        let state = test_state();
        let request = BatchScrapeRequest {
            urls: vec![],
            formats: default_formats(),
            only_main_content: true,
            timeout: 30000,
            headers: HashMap::new(),
            engine: "auto".to_string(),
            webhook_url: None,
        };

        let (status, _) = async_batch_scrape_handler(State(state), Json(request)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_async_too_many_urls() {
        let state = test_state();
        let urls: Vec<String> = (0..101)
            .map(|i| format!("https://example.com/page{}", i))
            .collect();
        let request = BatchScrapeRequest {
            urls,
            formats: default_formats(),
            only_main_content: true,
            timeout: 30000,
            headers: HashMap::new(),
            engine: "auto".to_string(),
            webhook_url: None,
        };

        let (status, _) = async_batch_scrape_handler(State(state), Json(request)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_async_returns_202() {
        let state = test_state();
        let request = BatchScrapeRequest {
            urls: vec!["https://example.com".to_string()],
            formats: default_formats(),
            only_main_content: true,
            timeout: 30000,
            headers: HashMap::new(),
            engine: "auto".to_string(),
            webhook_url: None,
        };

        let (status, body) = async_batch_scrape_handler(State(state), Json(request)).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body.0["success"], true);
        assert!(body.0["batchId"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_async_batch_status_not_found() {
        let state = test_state();
        let result =
            async_batch_status_handler(State(state), Path("nonexistent".to_string())).await;
        assert!(result.is_err());
    }
}
