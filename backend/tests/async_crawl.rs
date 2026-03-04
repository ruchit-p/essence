// Integration tests for async crawl lifecycle
//
// These tests verify the full async crawl flow: create → poll → results,
// cancellation, failure handling, and job listing.
//
// Tests marked #[ignore] require network access and a running server.
// Tests without #[ignore] use mockito and in-process routing.

mod api;

use api::make_request;
use axum::http::StatusCode;
use essence::api::AppState;
use essence::jobs::store::{JobStore, JobStoreConfig};
use serde_json::json;

fn create_test_app() -> axum::Router {
    let job_store = JobStore::new(JobStoreConfig {
        max_jobs: 50,
        result_ttl_secs: 10,
        cleanup_interval_secs: 2,
    });
    let state = AppState { job_store };
    essence::api::create_router(state)
}

// ============================================================================
// VALIDATION TESTS (no network needed)
// ============================================================================

#[tokio::test]
async fn test_async_crawl_invalid_url() {
    let app = create_test_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/crawl/async",
        Some(json!({
            "url": "not-a-valid-url"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["success"], false);
}

#[tokio::test]
async fn test_async_crawl_missing_url() {
    let app = create_test_app();

    let (status, _json) = make_request(
        app,
        "POST",
        "/api/v1/crawl/async",
        Some(json!({})),
    )
    .await;

    // Missing required field should fail deserialization
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_async_crawl_status_not_found() {
    let app = create_test_app();

    let (status, json) = make_request(
        app,
        "GET",
        "/api/v1/crawl/async/nonexistent-id",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["success"], false);
}

#[tokio::test]
async fn test_cancel_nonexistent_job() {
    let app = create_test_app();

    let (status, json) = make_request(
        app,
        "DELETE",
        "/api/v1/crawl/async/nonexistent-id",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["success"], false);
}

#[tokio::test]
async fn test_list_async_crawls_empty() {
    let app = create_test_app();

    let (status, json) = make_request(
        app,
        "GET",
        "/api/v1/crawl/async",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    assert!(json["data"].is_array());
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}

// ============================================================================
// LIFECYCLE TESTS (require network)
// ============================================================================

#[tokio::test]
#[ignore = "requires network"]
async fn test_async_crawl_full_lifecycle() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    // 1. Start async crawl
    let response = client
        .post(format!("{}/api/v1/crawl/async", base))
        .json(&json!({
            "url": "https://httpbin.org",
            "limit": 2,
            "maxDepth": 0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["success"], true);
    let crawl_id = body["crawlId"].as_str().unwrap();
    assert!(!crawl_id.is_empty());

    // 2. Poll until completion (max 60s)
    let mut final_body = json!(null);
    for _ in 0..60 {
        let poll = client
            .get(format!("{}/api/v1/crawl/async/{}", base, crawl_id))
            .send()
            .await
            .unwrap();
        assert_eq!(poll.status(), 200);

        let poll_body: serde_json::Value = poll.json().await.unwrap();
        let status = poll_body["status"].as_str().unwrap().to_string();

        if status == "completed" || status == "failed" {
            final_body = poll_body;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    assert_eq!(final_body["status"], "completed");
    assert!(final_body["data"].is_array());
    assert!(final_body["totalPages"].as_u64().unwrap() > 0);
}

#[tokio::test]
#[ignore = "requires network"]
async fn test_async_crawl_cancellation() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    // Start a crawl with high limit (will take time)
    let response = client
        .post(format!("{}/api/v1/crawl/async", base))
        .json(&json!({
            "url": "https://quotes.toscrape.com",
            "limit": 100,
            "maxDepth": 3
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    let crawl_id = body["crawlId"].as_str().unwrap();

    // Wait briefly for it to start
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Cancel it
    let cancel = client
        .delete(format!("{}/api/v1/crawl/async/{}", base, crawl_id))
        .send()
        .await
        .unwrap();

    assert_eq!(cancel.status(), 200);
    let cancel_body: serde_json::Value = cancel.json().await.unwrap();
    assert_eq!(cancel_body["success"], true);

    // Verify status is cancelled
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let status = client
        .get(format!("{}/api/v1/crawl/async/{}", base, crawl_id))
        .send()
        .await
        .unwrap();
    let status_body: serde_json::Value = status.json().await.unwrap();
    assert!(
        status_body["status"] == "cancelled" || status_body["status"] == "completed",
        "Expected cancelled or completed, got: {}",
        status_body["status"]
    );
}

#[tokio::test]
#[ignore = "requires network"]
async fn test_async_crawl_job_list() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    // Create two async crawls
    for _ in 0..2 {
        let response = client
            .post(format!("{}/api/v1/crawl/async", base))
            .json(&json!({
                "url": "https://example.com",
                "limit": 1,
                "maxDepth": 0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 202);
    }

    // Wait for them to finish
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // List jobs
    let list = client
        .get(format!("{}/api/v1/crawl/async", base))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);

    let list_body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(list_body["success"], true);
    assert!(list_body["data"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
#[ignore = "requires network"]
async fn test_existing_sync_crawl_still_works() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    let response = client
        .post(format!("{}/api/v1/crawl", base))
        .json(&json!({
            "url": "https://example.com",
            "limit": 1,
            "maxDepth": 0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["data"].is_array());
}
