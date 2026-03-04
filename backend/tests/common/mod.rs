// Common test utilities shared across test modules

use serde_json::{json, Value};
use std::collections::HashMap;

/// Mock data generators for tests
pub mod generators {
    use super::*;

    /// Generate a mock scrape response
    pub fn mock_scrape_response(url: &str, markdown: &str) -> Value {
        json!({
            "success": true,
            "data": {
                "markdown": markdown,
                "metadata": {
                    "title": "Test Page",
                    "description": "Test description",
                    "url": url,
                    "statusCode": 200
                },
                "links": [],
                "images": []
            }
        })
    }

    /// Generate mock HTML document
    pub fn mock_html(title: &str, body: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <meta name="description" content="Test description">
</head>
<body>
    {}
</body>
</html>"#,
            title, body
        )
    }

    /// Generate mock article with metadata
    pub fn mock_article_html(title: &str, content: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <meta name="description" content="Test article description">
    <meta property="og:title" content="{}">
    <meta property="og:description" content="OG description">
    <meta property="og:image" content="https://example.com/og-image.jpg">
</head>
<body>
    <article>
        <h1>{}</h1>
        {}
    </article>
</body>
</html>"#,
            title, title, title, content
        )
    }

    /// Generate URLs for testing
    pub fn generate_test_urls(count: usize) -> Vec<String> {
        (0..count)
            .map(|i| format!("https://example.com/page-{}", i))
            .collect()
    }

    /// Generate mock link data
    pub fn mock_links(count: usize) -> Vec<HashMap<String, String>> {
        (0..count)
            .map(|i| {
                let mut link = HashMap::new();
                link.insert("url".to_string(), format!("https://example.com/link-{}", i));
                link.insert("text".to_string(), format!("Link {}", i));
                link
            })
            .collect()
    }

    /// Generate mock image data
    pub fn mock_images(count: usize) -> Vec<HashMap<String, String>> {
        (0..count)
            .map(|i| {
                let mut img = HashMap::new();
                img.insert("url".to_string(), format!("https://example.com/image-{}.jpg", i));
                img.insert("alt".to_string(), format!("Image {}", i));
                img
            })
            .collect()
    }
}

/// Assertion helpers for common test patterns
pub mod assertions {
    use super::*;

    /// Assert that value is a successful response
    pub fn assert_success(response: &Value) {
        assert_eq!(
            response["success"].as_bool(),
            Some(true),
            "Expected success=true, got: {:?}",
            response
        );
    }

    /// Assert that value is an error response
    pub fn assert_error(response: &Value) {
        assert_eq!(
            response["success"].as_bool(),
            Some(false),
            "Expected success=false, got: {:?}",
            response
        );
    }

    /// Assert that response contains expected fields
    pub fn assert_has_fields(value: &Value, fields: &[&str]) {
        for field in fields {
            assert!(
                value.get(field).is_some(),
                "Missing expected field: {}",
                field
            );
        }
    }

    /// Assert that markdown content is not empty
    pub fn assert_markdown_not_empty(response: &Value) {
        let markdown = response["data"]["markdown"].as_str();
        assert!(markdown.is_some(), "Markdown field is missing");
        assert!(!markdown.unwrap().is_empty(), "Markdown is empty");
    }

    /// Assert that metadata contains expected keys
    pub fn assert_metadata_keys(response: &Value, keys: &[&str]) {
        let metadata = &response["data"]["metadata"];
        assert!(metadata.is_object(), "Metadata is not an object");

        for key in keys {
            assert!(
                metadata.get(key).is_some(),
                "Missing metadata key: {}",
                key
            );
        }
    }

    /// Assert minimum link count
    pub fn assert_min_links(response: &Value, min_count: usize) {
        let links = response["data"]["links"].as_array();
        assert!(links.is_some(), "Links field is missing");
        let link_count = links.unwrap().len();
        assert!(
            link_count >= min_count,
            "Expected at least {} links, got {}",
            min_count,
            link_count
        );
    }

    /// Assert minimum image count
    pub fn assert_min_images(response: &Value, min_count: usize) {
        let images = response["data"]["images"].as_array();
        assert!(images.is_some(), "Images field is missing");
        let image_count = images.unwrap().len();
        assert!(
            image_count >= min_count,
            "Expected at least {} images, got {}",
            min_count,
            image_count
        );
    }

    /// Assert HTTP status code
    pub fn assert_status_code(response: &Value, expected: u16) {
        let status = response["data"]["metadata"]["statusCode"].as_u64();
        assert!(status.is_some(), "Status code field is missing");
        assert_eq!(
            status.unwrap(),
            expected as u64,
            "Expected status {}, got {}",
            expected,
            status.unwrap()
        );
    }
}

/// Test fixtures and helpers
pub mod fixtures {
    use std::fs;
    use std::path::PathBuf;

    /// Load a test fixture file
    pub fn load_fixture(filename: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/fixtures");
        path.push(filename);
        fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to load fixture: {}", filename))
    }

    /// Load JSON fixture
    pub fn load_json_fixture(filename: &str) -> serde_json::Value {
        let content = load_fixture(filename);
        serde_json::from_str(&content)
            .unwrap_or_else(|_| panic!("Failed to parse JSON fixture: {}", filename))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_scrape_response() {
        let response = generators::mock_scrape_response("https://example.com", "# Test");
        assert_eq!(response["success"], true);
        assert_eq!(response["data"]["markdown"], "# Test");
    }

    #[test]
    fn test_mock_html_generation() {
        let html = generators::mock_html("Test", "<p>Content</p>");
        assert!(html.contains("<title>Test</title>"));
        assert!(html.contains("<p>Content</p>"));
    }

    #[test]
    fn test_generate_test_urls() {
        let urls = generators::generate_test_urls(5);
        assert_eq!(urls.len(), 5);
        assert_eq!(urls[0], "https://example.com/page-0");
        assert_eq!(urls[4], "https://example.com/page-4");
    }

    #[test]
    fn test_assert_success() {
        let response = json!({"success": true});
        assertions::assert_success(&response);
    }

    #[test]
    #[should_panic]
    fn test_assert_success_fails_on_error() {
        let response = json!({"success": false});
        assertions::assert_success(&response);
    }

    #[test]
    fn test_assert_has_fields() {
        let value = json!({"field1": "value1", "field2": "value2"});
        assertions::assert_has_fields(&value, &["field1", "field2"]);
    }
}
