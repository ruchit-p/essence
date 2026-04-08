use tokio::time::Duration;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn test_retry_on_500_error() {
    let mock_server = MockServer::start().await;

    // First two requests return 500, third succeeds
    Mock::given(method("GET"))
        .and(path("/test-500"))
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-500"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html><body>Success</body></html>"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/test-500", mock_server.uri());

    // Create HTTP client with retry logic
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // Attempt with retry logic
    let mut attempts = 0;
    let mut response = None;

    while attempts < 3 {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                response = Some(resp);
                break;
            }
            _ => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    assert!(response.is_some(), "Should succeed after retries");
    assert_eq!(attempts, 2, "Should retry exactly 2 times before success");
}

#[tokio::test]
async fn test_retry_on_timeout() {
    let mock_server = MockServer::start().await;

    // First request delays, second succeeds
    Mock::given(method("GET"))
        .and(path("/test-timeout"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-timeout"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html><body>Success</body></html>"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/test-timeout", mock_server.uri());

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    // First attempt should timeout
    let first_attempt = client.get(&url).send().await;
    assert!(first_attempt.is_err(), "First attempt should timeout");

    // Second attempt should succeed
    let second_attempt = client.get(&url).send().await;
    assert!(second_attempt.is_ok(), "Second attempt should succeed");
}

#[tokio::test]
async fn test_successful_scrape_no_retry() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/test-success"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"<html>
                <head><title>Test Page</title></head>
                <body><h1>Hello World</h1></body>
            </html>"#,
        ))
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/test-success", mock_server.uri());

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    assert!(body.contains("Test Page"));
    assert!(body.contains("Hello World"));
}

#[tokio::test]
async fn test_max_retries_exceeded() {
    let mock_server = MockServer::start().await;

    // Always return 500
    Mock::given(method("GET"))
        .and(path("/test-always-fail"))
        .respond_with(ResponseTemplate::new(500))
        .expect(3)
        .mount(&mock_server)
        .await;

    let url = format!("{}/test-always-fail", mock_server.uri());

    let client = reqwest::Client::new();
    let max_retries = 3;
    let mut attempts = 0;
    let mut success = false;

    while attempts < max_retries {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                success = true;
                break;
            }
            _ => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    assert!(!success, "Should not succeed after max retries");
    assert_eq!(
        attempts, max_retries,
        "Should attempt exactly max_retries times"
    );
}

#[tokio::test]
async fn test_exponential_backoff() {
    let mock_server = MockServer::start().await;

    // Fail twice, then succeed
    Mock::given(method("GET"))
        .and(path("/test-backoff"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test-backoff"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html><body>Success</body></html>"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/test-backoff", mock_server.uri());
    let client = reqwest::Client::new();

    let mut attempts = 0;
    let mut response = None;
    let start = std::time::Instant::now();

    while attempts < 3 {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                response = Some(resp);
                break;
            }
            _ => {
                attempts += 1;
                // Exponential backoff: 100ms, 200ms, 400ms
                let delay = Duration::from_millis(100 * 2_u64.pow(attempts - 1));
                tokio::time::sleep(delay).await;
            }
        }
    }

    let elapsed = start.elapsed();

    assert!(response.is_some(), "Should eventually succeed");
    // Should have waited at least 300ms (100ms + 200ms)
    assert!(
        elapsed >= Duration::from_millis(300),
        "Should use exponential backoff"
    );
}

#[tokio::test]
async fn test_retry_on_connection_error() {
    // Use an invalid port to simulate connection error
    let url = "http://localhost:9999/test";

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    let mut attempts = 0;
    let max_retries = 3;

    while attempts < max_retries {
        match client.get(url).send().await {
            Ok(_) => break,
            Err(_) => {
                attempts += 1;
                if attempts < max_retries {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    assert_eq!(attempts, max_retries, "Should retry on connection errors");
}

#[tokio::test]
async fn test_custom_headers_with_retry() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/test-headers"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html><body>Headers OK</body></html>"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let url = format!("{}/test-headers", mock_server.uri());

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "EssenceBot/1.0")
        .header("Accept", "text/html")
        .send()
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status(), 200);
}
