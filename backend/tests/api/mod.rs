// Test utilities and helpers for API integration tests

pub mod crawl_test;
pub mod metrics;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::Value;
use tower::ServiceExt;

/// Test helper to create a mock server for testing
pub mod mock_server {
    use mockito::{Server, ServerGuard};

    pub async fn create() -> ServerGuard {
        Server::new_async().await
    }
}

/// Test helper to load fixture files
pub mod fixtures {
    use std::fs;
    use std::path::PathBuf;

    /// Load an HTML fixture file
    pub fn load_html(filename: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("fixtures/test");
        path.push(filename);
        fs::read_to_string(path).unwrap_or_else(|_| panic!("Failed to load fixture: {}", filename))
    }

    /// Load robots.txt fixture
    pub fn load_robots_txt() -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("fixtures/test/robots.txt");
        fs::read_to_string(path).expect("Failed to load robots.txt fixture")
    }
}

/// Test helper to make API requests
pub async fn make_request(
    app: Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = match body {
        Some(json_body) => {
            let body_str = serde_json::to_string(&json_body).unwrap();
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body_str))
                .unwrap()
        }
        None => Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap(),
    };

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    let json: Value = serde_json::from_str(&body_str).unwrap_or(Value::Null);

    (status, json)
}

/// Create the test application router
pub fn create_app() -> Router {
    essence::api::create_router()
}

/// Helper to send a scrape request
pub async fn send_scrape_request(app: &Router, payload: Value) -> Value {
    let (_status, json) = make_request(app.clone(), "POST", "/api/v1/scrape", Some(payload)).await;
    json
}

/// Helper to send a map request
pub async fn send_map_request(app: &Router, payload: Value) -> Value {
    let (_status, json) = make_request(app.clone(), "POST", "/api/v1/map", Some(payload)).await;
    json
}

/// Helper to load a fixture file (alias for fixtures::load_html)
pub fn load_fixture(filename: &str) -> String {
    fixtures::load_html(filename)
}

/// Test assertions
pub mod assertions {
    use serde_json::Value;

    /// Assert that response has success=true
    pub fn assert_success(json: &Value) {
        assert!(
            json["success"].as_bool().unwrap_or(false),
            "Expected success=true, got: {}",
            json
        );
    }

    /// Assert that response has success=false
    pub fn assert_error(json: &Value) {
        assert!(
            !json["success"].as_bool().unwrap_or(true),
            "Expected success=false, got: {}",
            json
        );
    }

    /// Assert that response contains a specific field
    pub fn assert_has_field(json: &Value, field: &str) {
        assert!(
            json.get(field).is_some(),
            "Expected field '{}' not found in: {}",
            field,
            json
        );
    }

    /// Assert that response data contains a specific field
    pub fn assert_data_has_field(json: &Value, field: &str) {
        assert!(
            json["data"].get(field).is_some(),
            "Expected data field '{}' not found in: {}",
            field,
            json
        );
    }

    /// Assert markdown content is not empty
    pub fn assert_markdown_not_empty(json: &Value) {
        let markdown = json["data"]["markdown"].as_str().unwrap_or("");
        assert!(!markdown.is_empty(), "Markdown content is empty");
    }

    /// Assert metadata extraction
    pub fn assert_metadata_extracted(json: &Value) {
        assert_has_field(&json["data"], "metadata");
        let metadata = &json["data"]["metadata"];
        assert!(metadata.is_object(), "Metadata is not an object");
    }

    /// Assert links were extracted
    pub fn assert_links_extracted(json: &Value, min_count: usize) {
        let empty_vec = vec![];
        let links = json["data"]["links"].as_array().unwrap_or(&empty_vec);
        assert!(
            links.len() >= min_count,
            "Expected at least {} links, got {}",
            min_count,
            links.len()
        );
    }

    /// Assert images were extracted
    pub fn assert_images_extracted(json: &Value, min_count: usize) {
        let empty_vec = vec![];
        let images = json["data"]["images"].as_array().unwrap_or(&empty_vec);
        assert!(
            images.len() >= min_count,
            "Expected at least {} images, got {}",
            min_count,
            images.len()
        );
    }

    /// Assert that response has markdown content
    pub fn assert_has_markdown(json: &Value) {
        assert_has_field(&json["data"], "markdown");
        assert_markdown_not_empty(json);
    }

    /// Assert that response has metadata with specific fields
    pub fn assert_has_metadata(json: &Value, fields: &[&str]) {
        assert_metadata_extracted(json);
        for field in fields {
            assert!(
                json["data"]["metadata"].get(field).is_some(),
                "Expected metadata field '{}' not found",
                field
            );
        }
    }

    /// Assert that response has links
    pub fn assert_has_links(json: &Value) {
        assert_has_field(&json["data"], "links");
        let empty_vec = vec![];
        let links = json["data"]["links"].as_array().unwrap_or(&empty_vec);
        assert!(!links.is_empty(), "Expected links array to not be empty");
    }

    /// Assert that response has images
    pub fn assert_has_images(json: &Value) {
        assert_has_field(&json["data"], "images");
        let empty_vec = vec![];
        let images = json["data"]["images"].as_array().unwrap_or(&empty_vec);
        assert!(!images.is_empty(), "Expected images array to not be empty");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_html_fixture() {
        let html = fixtures::load_html("simple.html");
        assert!(html.contains("<title>Simple Test Page</title>"));
    }

    #[test]
    fn test_load_robots_txt() {
        let robots = fixtures::load_robots_txt();
        assert!(robots.contains("User-agent:"));
    }
}
