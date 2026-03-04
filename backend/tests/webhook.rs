// Integration tests for webhook delivery
//
// All tests require network access and a running server.

use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Start a mock webhook receiver on a random port.
/// Returns (port, received_bodies) where received_bodies is shared state
/// that accumulates all POSTed JSON bodies.
async fn start_webhook_receiver() -> (u16, Arc<Mutex<Vec<serde_json::Value>>>) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();

    let app = axum::Router::new().route(
        "/webhook",
        axum::routing::post({
            let received = received_clone;
            move |axum::Json(body): axum::Json<serde_json::Value>| {
                let received = received.clone();
                async move {
                    received.lock().await.push(body);
                    axum::http::StatusCode::OK
                }
            }
        }),
    );

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Brief wait for server to bind
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    (port, received)
}

#[tokio::test]
#[ignore = "requires network and running server"]
async fn test_webhook_delivery_on_crawl_complete() {
    let (webhook_port, received) = start_webhook_receiver().await;
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    // Start crawl with webhook
    let response = client
        .post(format!("{}/api/v1/crawl/async", base))
        .json(&json!({
            "url": "https://example.com",
            "limit": 1,
            "maxDepth": 0,
            "webhookUrl": format!("http://127.0.0.1:{}/webhook", webhook_port)
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 202);
    let body: serde_json::Value = response.json().await.unwrap();
    let crawl_id = body["crawlId"].as_str().unwrap().to_string();

    // Wait for webhook delivery (max 60s)
    for _ in 0..60 {
        let bodies = received.lock().await;
        if !bodies.is_empty() {
            break;
        }
        drop(bodies);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let bodies = received.lock().await;
    assert!(
        !bodies.is_empty(),
        "Expected webhook to be delivered within 60s"
    );

    let webhook_body = &bodies[0];
    assert!(
        webhook_body["event"] == "crawl.completed" || webhook_body["event"] == "crawl.failed",
        "Expected crawl event, got: {}",
        webhook_body["event"]
    );
    assert_eq!(webhook_body["crawlId"], crawl_id);

    if webhook_body["event"] == "crawl.completed" {
        assert!(webhook_body["data"].is_array());
    }
}

#[tokio::test]
#[ignore = "requires network and running server"]
async fn test_webhook_delivery_on_batch_complete() {
    let (webhook_port, received) = start_webhook_receiver().await;
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    // Start async batch scrape with webhook
    let response = client
        .post(format!("{}/api/v1/batch/scrape/async", base))
        .json(&json!({
            "urls": ["https://example.com"],
            "webhookUrl": format!("http://127.0.0.1:{}/webhook", webhook_port)
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 202);

    // Wait for webhook delivery (max 60s)
    for _ in 0..60 {
        let bodies = received.lock().await;
        if !bodies.is_empty() {
            break;
        }
        drop(bodies);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let bodies = received.lock().await;
    assert!(
        !bodies.is_empty(),
        "Expected webhook to be delivered within 60s"
    );

    let webhook_body = &bodies[0];
    assert!(
        webhook_body["event"] == "batch_scrape.completed"
            || webhook_body["event"] == "batch_scrape.failed",
        "Expected batch_scrape event, got: {}",
        webhook_body["event"]
    );
}

#[tokio::test]
#[ignore = "requires network and running server"]
async fn test_no_webhook_when_not_configured() {
    let (webhook_port, received) = start_webhook_receiver().await;
    let _ = webhook_port; // Unused but receiver is running
    let client = reqwest::Client::new();
    let base = "http://localhost:8080";

    // Start crawl WITHOUT webhook
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
    let body: serde_json::Value = response.json().await.unwrap();
    let crawl_id = body["crawlId"].as_str().unwrap();

    // Wait for crawl to complete
    for _ in 0..60 {
        let poll = client
            .get(format!("{}/api/v1/crawl/async/{}", base, crawl_id))
            .send()
            .await
            .unwrap();
        let poll_body: serde_json::Value = poll.json().await.unwrap();
        if poll_body["status"] == "completed" || poll_body["status"] == "failed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    // Webhook receiver should have received nothing
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let bodies = received.lock().await;
    assert!(
        bodies.is_empty(),
        "Expected no webhook delivery, but got {} deliveries",
        bodies.len()
    );
}
