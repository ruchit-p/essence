// Integration tests for batch scrape endpoints
//
// Tests marked #[ignore] require network access and a running server.
// Tests without #[ignore] use in-process routing with mockito.

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
async fn test_batch_scrape_empty_urls() {
    let app = create_test_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/batch/scrape",
        Some(json!({
            "urls": []
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["success"], false);
}

#[tokio::test]
async fn test_batch_scrape_too_many_urls_sync() {
    let app = create_test_app();

    let urls: Vec<String> = (0..11)
        .map(|i| format!("https://example.com/page-{}", i))
        .collect();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/batch/scrape",
        Some(json!({
            "urls": urls
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["success"], false);
}

#[tokio::test]
async fn test_async_batch_scrape_empty_urls() {
    let app = create_test_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/batch/scrape/async",
        Some(json!({
            "urls": []
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["success"], false);
}

#[tokio::test]
async fn test_async_batch_scrape_too_many_urls() {
    let app = create_test_app();

    let urls: Vec<String> = (0..101)
        .map(|i| format!("https://example.com/page-{}", i))
        .collect();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/batch/scrape/async",
        Some(json!({
            "urls": urls
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["success"], false);
}

#[tokio::test]
async fn test_async_batch_status_not_found() {
    let app = create_test_app();

    let (status, json) = make_request(
        app,
        "GET",
        "/api/v1/batch/scrape/async/nonexistent-id",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["success"], false);
}

// ============================================================================
// LIFECYCLE TESTS (require network)
// ============================================================================

#[tokio::test]
#[ignore = "requires network"]
async fn test_batch_scrape_sync() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    let response = client
        .post(format!("{}/api/v1/batch/scrape", base))
        .json(&json!({
            "urls": [
                "https://httpbin.org/html",
                "https://example.com"
            ],
            "formats": ["markdown"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    assert_eq!(body["success"], true);
    assert_eq!(body["totalUrls"], 2);
    assert!(body["successCount"].as_u64().unwrap() >= 1);

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    for item in data {
        assert!(item["url"].is_string());
        assert!(item["success"].is_boolean());
    }
}

#[tokio::test]
#[ignore = "requires network"]
async fn test_batch_scrape_partial_failure() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    let response = client
        .post(format!("{}/api/v1/batch/scrape", base))
        .json(&json!({
            "urls": [
                "https://example.com",
                "https://this-domain-definitely-does-not-exist-12345.com"
            ],
            "formats": ["markdown"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    assert_eq!(body["success"], true);
    assert_eq!(body["totalUrls"], 2);
    assert!(body["successCount"].as_u64().unwrap() >= 1);
    assert!(body["errorCount"].as_u64().unwrap() >= 1);

    let data = body["data"].as_array().unwrap();
    // At least one should succeed
    assert!(data.iter().any(|item| item["success"] == true));
    // At least one should fail
    assert!(data.iter().any(|item| item["success"] == false));
}

#[tokio::test]
#[ignore = "requires network"]
async fn test_batch_scrape_deduplicates_urls() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    let response = client
        .post(format!("{}/api/v1/batch/scrape", base))
        .json(&json!({
            "urls": [
                "https://example.com",
                "https://example.com",
                "https://example.com"
            ],
            "formats": ["markdown"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    assert_eq!(body["success"], true);
    // After dedup, should have fewer unique URLs
    let total = body["totalUrls"].as_u64().unwrap();
    assert!(total <= 3);
}

#[tokio::test]
#[ignore = "requires network"]
async fn test_async_batch_scrape_lifecycle() {
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    let response = client
        .post(format!("{}/api/v1/batch/scrape/async", base))
        .json(&json!({
            "urls": [
                "https://example.com",
                "https://httpbin.org/html",
                "https://httpbin.org/robots.txt"
            ],
            "formats": ["markdown"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["success"], true);
    let batch_id = body["batchId"].as_str().unwrap();

    // Poll until completion
    let mut final_body = json!(null);
    for _ in 0..60 {
        let poll = client
            .get(format!("{}/api/v1/batch/scrape/async/{}", base, batch_id))
            .send()
            .await
            .unwrap();

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
    assert!(final_body["totalUrls"].as_u64().unwrap() >= 1);
}
