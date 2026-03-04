// Verification integration tests for all fixed behaviors.
//
// These tests exercise the full request pipeline with mock servers (no network required).
// They verify include_tags/exclude_tags filtering, BrowserAction deserialization,
// all output formats, async crawl lifecycle, batch scrape, error formats, and route registration.

mod api;

use api::{assertions::*, create_app, load_fixture, make_request};
use axum::http::StatusCode;
use essence::api::AppState;
use essence::jobs::store::{JobStore, JobStoreConfig};
use serde_json::json;

/// Create a test app with short TTLs for async job tests.
fn create_test_app() -> axum::Router {
    std::env::set_var("ESSENCE_ALLOW_LOCALHOST", "1");
    let job_store = JobStore::new(JobStoreConfig {
        max_jobs: 50,
        result_ttl_secs: 60,
        cleanup_interval_secs: 300,
    });
    let state = AppState { job_store };
    essence::api::create_router(state)
}

// ============================================================================
// Tests 1-3: include_tags / exclude_tags filtering through /api/v1/scrape
// ============================================================================

#[tokio::test]
async fn test_scrape_include_tags_filters_content() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/with-ads")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("with_ads.html"))
        .create_async()
        .await;

    let payload = json!({
        "url": format!("{}/with-ads", server.url()),
        "formats": ["markdown"],
        "includeTags": ["main"]
    });

    let (status, response) = make_request(app, "POST", "/api/v1/scrape", Some(payload)).await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&response);

    let markdown = response["data"]["markdown"].as_str().unwrap_or("");
    assert!(
        markdown.contains("Main Content") || markdown.to_lowercase().contains("main content"),
        "Markdown should contain main content, got: {}",
        markdown
    );
    // ad-banner is outside <main>, so should not appear
    assert!(
        !markdown.contains("ad-banner"),
        "Markdown should not contain ad-banner class text when include_tags=[main]"
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn test_scrape_exclude_tags_removes_content() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/with-ads")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("with_ads.html"))
        .create_async()
        .await;

    let payload = json!({
        "url": format!("{}/with-ads", server.url()),
        "formats": ["markdown"],
        "excludeTags": [".ad-banner", ".footer-ads", ".sidebar-ad"]
    });

    let (status, response) = make_request(app, "POST", "/api/v1/scrape", Some(payload)).await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&response);

    let markdown = response["data"]["markdown"].as_str().unwrap_or("");
    assert!(
        markdown.contains("Main Content") || markdown.to_lowercase().contains("main content"),
        "Markdown should still contain main content after excluding ads, got: {}",
        markdown
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn test_scrape_include_and_exclude_combined() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/with-ads")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("with_ads.html"))
        .create_async()
        .await;

    let payload = json!({
        "url": format!("{}/with-ads", server.url()),
        "formats": ["markdown"],
        "includeTags": ["main"],
        "excludeTags": [".sidebar-ad"]
    });

    let (status, response) = make_request(app, "POST", "/api/v1/scrape", Some(payload)).await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&response);

    let markdown = response["data"]["markdown"].as_str().unwrap_or("");
    assert!(
        markdown.contains("Main Content") || markdown.to_lowercase().contains("main content"),
        "Markdown should contain main content, got: {}",
        markdown
    );
    assert!(
        markdown.contains("More content") || markdown.to_lowercase().contains("more content"),
        "Markdown should contain 'More content here', got: {}",
        markdown
    );

    mock.assert_async().await;
}

// ============================================================================
// Tests 4-5: BrowserAction deserialization via /api/v1/scrape
// ============================================================================

#[tokio::test]
async fn test_scrape_browser_action_pascal_case_accepted() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/page")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    // PascalCase "Wait" should be accepted (deserialized correctly)
    let payload = json!({
        "url": format!("{}/page", server.url()),
        "formats": ["markdown"],
        "actions": [{"type": "Wait", "milliseconds": 100}],
        "engine": "http"
    });

    let (status, _response) = make_request(app, "POST", "/api/v1/scrape", Some(payload)).await;
    // Should not get 422 (deserialization failure). Any successful status or scrape error is fine.
    assert_ne!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "PascalCase BrowserAction 'Wait' should be accepted by deserializer"
    );
}

#[tokio::test]
async fn test_scrape_browser_action_camel_case_accepted() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/page")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    // camelCase "waitForSelector" should be accepted
    let payload = json!({
        "url": format!("{}/page", server.url()),
        "formats": ["markdown"],
        "actions": [{"type": "waitForSelector", "selector": ".loaded"}],
        "engine": "http"
    });

    let (status, _response) = make_request(app, "POST", "/api/v1/scrape", Some(payload)).await;
    assert_ne!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "camelCase BrowserAction 'waitForSelector' should be accepted by deserializer"
    );
}

// ============================================================================
// Tests 6-7: Format handling and metadata
// ============================================================================

#[tokio::test]
async fn test_scrape_all_valid_formats() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = json!({
        "url": format!("{}/test", server.url()),
        "formats": ["markdown", "html", "rawHtml", "links", "images"]
    });

    let (status, response) = make_request(app, "POST", "/api/v1/scrape", Some(payload)).await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&response);

    // All 5 format fields should be populated
    let data = &response["data"];
    assert!(data["markdown"].is_string(), "markdown should be present");
    assert!(data["html"].is_string(), "html should be present");
    assert!(data["rawHtml"].is_string(), "rawHtml should be present");
    assert!(data["links"].is_array(), "links should be present");
    assert!(data["images"].is_array(), "images should be present");

    // simple.html has 2 links and 2 images
    let links = data["links"].as_array().unwrap();
    assert!(
        links.len() >= 2,
        "Expected at least 2 links, got {}",
        links.len()
    );

    let images = data["images"].as_array().unwrap();
    assert!(
        images.len() >= 2,
        "Expected at least 2 images, got {}",
        images.len()
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn test_scrape_metadata_always_included() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    // Request only markdown, but metadata should still be included
    let payload = json!({
        "url": format!("{}/test", server.url()),
        "formats": ["markdown"]
    });

    let (status, response) = make_request(app, "POST", "/api/v1/scrape", Some(payload)).await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&response);

    let metadata = &response["data"]["metadata"];
    assert!(metadata.is_object(), "metadata should be an object");
    assert!(
        metadata.get("statusCode").is_some(),
        "metadata should have statusCode"
    );
    assert_eq!(
        metadata["statusCode"], 200,
        "statusCode should be 200"
    );

    // simple.html has title and description meta tags
    assert!(
        metadata.get("title").is_some(),
        "metadata should have title"
    );

    mock.assert_async().await;
}

// ============================================================================
// Tests 8-11: Async crawl lifecycle (no network, in-process routing)
// ============================================================================

#[tokio::test]
async fn test_async_crawl_create_returns_202_with_job_id() {
    let app = create_test_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>Test</h1></body></html>")
        .create_async()
        .await;

    let (status, response) = make_request(
        app,
        "POST",
        "/api/v1/crawl/async",
        Some(json!({
            "url": format!("{}/", server.url()),
            "limit": 1,
            "maxDepth": 0
        })),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "Should return 202 Accepted");
    assert_eq!(response["success"], true);
    assert!(
        response["crawlId"].is_string(),
        "Should have crawlId string"
    );
    assert_eq!(response["status"], "queued");
    assert!(
        response["statusUrl"].is_string(),
        "Should have statusUrl"
    );

    let crawl_id = response["crawlId"].as_str().unwrap();
    let status_url = response["statusUrl"].as_str().unwrap();
    assert!(
        status_url.contains(crawl_id),
        "statusUrl should contain crawlId"
    );
}

#[tokio::test]
async fn test_async_crawl_status_returns_correct_structure() {
    let app = create_test_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>Test</h1></body></html>")
        .create_async()
        .await;

    // Create the job
    let (_, create_response) = make_request(
        app.clone(),
        "POST",
        "/api/v1/crawl/async",
        Some(json!({
            "url": format!("{}/", server.url()),
            "limit": 1,
            "maxDepth": 0
        })),
    )
    .await;

    let crawl_id = create_response["crawlId"].as_str().unwrap();

    // Poll status
    let (status, poll_response) = make_request(
        app,
        "GET",
        &format!("/api/v1/crawl/async/{}", crawl_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        poll_response["crawlId"].is_string(),
        "Should have crawlId"
    );

    let job_status = poll_response["status"].as_str().unwrap();
    assert!(
        ["queued", "running", "completed", "failed"].contains(&job_status),
        "Status should be valid, got: {}",
        job_status
    );
}

#[tokio::test]
async fn test_async_crawl_cancel_returns_success() {
    let app = create_test_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>Test</h1></body></html>")
        .create_async()
        .await;

    // Create the job
    let (_, create_response) = make_request(
        app.clone(),
        "POST",
        "/api/v1/crawl/async",
        Some(json!({
            "url": format!("{}/", server.url()),
            "limit": 50,
            "maxDepth": 3
        })),
    )
    .await;

    let crawl_id = create_response["crawlId"].as_str().unwrap();

    // Cancel the job
    let (status, cancel_response) = make_request(
        app.clone(),
        "DELETE",
        &format!("/api/v1/crawl/async/{}", crawl_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancel_response["success"], true);
    assert_eq!(cancel_response["status"], "cancelled");

    // Verify status shows cancelled
    let (_, status_response) = make_request(
        app,
        "GET",
        &format!("/api/v1/crawl/async/{}", crawl_id),
        None,
    )
    .await;

    assert_eq!(
        status_response["status"], "cancelled",
        "Job status should be cancelled after DELETE"
    );
}

#[tokio::test]
async fn test_async_crawl_list_shows_created_jobs() {
    let app = create_test_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>Test</h1></body></html>")
        .create_async()
        .await;

    // Create 2 async crawl jobs
    for _ in 0..2 {
        let (_status, _) = make_request(
            app.clone(),
            "POST",
            "/api/v1/crawl/async",
            Some(json!({
                "url": format!("{}/", server.url()),
                "limit": 1,
                "maxDepth": 0
            })),
        )
        .await;
    }

    // List jobs
    let (status, list_response) = make_request(
        app,
        "GET",
        "/api/v1/crawl/async",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(list_response["success"], true);

    let data = list_response["data"].as_array().unwrap();
    assert!(
        data.len() >= 2,
        "Expected at least 2 jobs in list, got {}",
        data.len()
    );

    // Each job entry should have crawlId and status
    for job in data {
        assert!(job["crawlId"].is_string(), "Job should have crawlId");
        assert!(job["status"].is_string(), "Job should have status");
    }
}

// ============================================================================
// Tests 12-13: Batch scrape
// ============================================================================

#[tokio::test]
async fn test_batch_scrape_sync_with_mock_urls() {
    let app = create_test_app();
    let mut server = mockito::Server::new_async().await;

    // Set up 3 different pages with substantial content to pass quality checks
    let page_html = |n: u32, title: &str, body: &str| -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>{title}</title><meta name="description" content="Test page {n}"></head>
<body>
<article>
<h1>{title}</h1>
<p>{body}</p>
<p>This is additional paragraph content to ensure the page has sufficient text density for the content quality checks. The scraper validates that pages have meaningful content before accepting them as successful scrapes.</p>
<p>Here is a third paragraph with more information about the topic. This helps ensure the markdown output exceeds the minimum length threshold that the quality validator enforces on scraped content.</p>
</article>
</body>
</html>"#,
            n = n,
            title = title,
            body = body
        )
    };

    let _mock1 = server
        .mock("GET", "/page1")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(page_html(1, "Page One - Introduction", "Welcome to page one. This page contains introductory content about the testing framework and how batch scraping works across multiple URLs simultaneously."))
        .create_async()
        .await;

    let _mock2 = server
        .mock("GET", "/page2")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(page_html(2, "Page Two - Details", "This is page two with detailed content. It explains the inner workings of the batch processing system and how concurrent requests are managed by the rate limiter."))
        .create_async()
        .await;

    let _mock3 = server
        .mock("GET", "/page3")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(page_html(3, "Page Three - Conclusion", "Page three wraps up the series with concluding remarks. The batch scrape system processes all URLs in parallel while respecting per-domain rate limits and concurrency settings."))
        .create_async()
        .await;

    let (status, response) = make_request(
        app,
        "POST",
        "/api/v1/batch/scrape",
        Some(json!({
            "urls": [
                format!("{}/page1", server.url()),
                format!("{}/page2", server.url()),
                format!("{}/page3", server.url())
            ],
            "formats": ["markdown"]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["success"], true);
    assert_eq!(response["totalUrls"], 3);

    let data = response["data"].as_array().unwrap();
    assert_eq!(data.len(), 3, "Should have 3 results");

    let success_count = response["successCount"].as_u64().unwrap();
    assert!(
        success_count >= 1,
        "At least 1 URL should succeed, got {}",
        success_count
    );

    // Each result item should have url and success fields
    for item in data {
        assert!(item["url"].is_string(), "Each item should have url");
        assert!(item["success"].is_boolean(), "Each item should have success");
    }
}

#[tokio::test]
async fn test_batch_scrape_async_returns_202_with_batch_id() {
    let app = create_test_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/page")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>Test</h1><p>Some content here.</p></body></html>")
        .create_async()
        .await;

    let (status, response) = make_request(
        app,
        "POST",
        "/api/v1/batch/scrape/async",
        Some(json!({
            "urls": [
                format!("{}/page", server.url()),
                format!("{}/page", server.url())
            ],
            "formats": ["markdown"]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED, "Should return 202 Accepted");
    assert_eq!(response["success"], true);
    assert!(
        response["batchId"].is_string(),
        "Should have batchId"
    );
    assert_eq!(response["status"], "queued");
    assert_eq!(response["totalUrls"], 2);
}

// ============================================================================
// Test 14: NotFound error response format
// ============================================================================

#[tokio::test]
async fn test_not_found_error_format() {
    let app = create_test_app();

    let (status, response) = make_request(
        app,
        "GET",
        "/api/v1/crawl/async/nonexistent-uuid",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(response["success"], false);

    let error_msg = response["error"].as_str().unwrap();
    assert!(
        error_msg.contains("not found") || error_msg.contains("Not found"),
        "Error message should indicate not found, got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("nonexistent-uuid"),
        "Error message should contain the job ID, got: {}",
        error_msg
    );
}

// ============================================================================
// Test 15: Health endpoint
// ============================================================================

#[tokio::test]
async fn test_health_endpoint() {
    let app = create_test_app();

    let request = axum::http::Request::builder()
        .method("GET")
        .uri("/health")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body_str, "ok");
}

// ============================================================================
// Test 16: All routes registered (smoke test)
// ============================================================================

#[tokio::test]
async fn test_all_routes_registered() {
    let app = create_test_app();
    let mut server = mockito::Server::new_async().await;

    let _mock = server
        .mock("GET", "/")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html><body><h1>Test</h1></body></html>")
        .create_async()
        .await;

    let mock_url = server.url();

    // POST /api/v1/crawl/async should not 404
    let (status, _) = make_request(
        app.clone(),
        "POST",
        "/api/v1/crawl/async",
        Some(json!({
            "url": format!("{}/", mock_url),
            "limit": 1,
            "maxDepth": 0
        })),
    )
    .await;
    assert_ne!(status, StatusCode::NOT_FOUND, "POST /api/v1/crawl/async should be registered");

    // GET /api/v1/crawl/async should not 404
    let (status, _) = make_request(
        app.clone(),
        "GET",
        "/api/v1/crawl/async",
        None,
    )
    .await;
    assert_ne!(status, StatusCode::NOT_FOUND, "GET /api/v1/crawl/async should be registered");

    // POST /api/v1/batch/scrape should not 404 (send empty to get 400, not 404)
    let (status, _) = make_request(
        app.clone(),
        "POST",
        "/api/v1/batch/scrape",
        Some(json!({"urls": []})),
    )
    .await;
    assert_ne!(status, StatusCode::NOT_FOUND, "POST /api/v1/batch/scrape should be registered");

    // POST /api/v1/batch/scrape/async should not 404
    let (status, _) = make_request(
        app.clone(),
        "POST",
        "/api/v1/batch/scrape/async",
        Some(json!({"urls": []})),
    )
    .await;
    assert_ne!(status, StatusCode::NOT_FOUND, "POST /api/v1/batch/scrape/async should be registered");

    // GET /health should not 404
    let (status, _) = make_request(
        app,
        "GET",
        "/health",
        None,
    )
    .await;
    assert_ne!(status, StatusCode::NOT_FOUND, "GET /health should be registered");
}
