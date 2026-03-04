// Integration tests for /api/v1/scrape endpoint
// Following TDD approach - these tests should FAIL until rust-scraper implements the features

use axum::http::StatusCode;
use serde_json::json;

mod api;
use api::{assertions::*, fixtures, make_request, mock_server};

// Helper to create the app router (will be implemented by rust-scraper)
fn create_app() -> axum::Router {
    // This will be replaced with actual app creation once implemented
    axum::Router::new()
}

// ============================================================================
// BASIC SCRAPING TESTS
// ============================================================================

#[tokio::test]
async fn test_scrape_basic_html() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["markdown"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_data_has_field(&json, "markdown");
    assert_markdown_not_empty(&json);
}

#[tokio::test]
async fn test_scrape_returns_title_and_description() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);

    // Check title extraction
    assert_eq!(
        json["data"]["title"].as_str().unwrap(),
        "Simple Test Page"
    );

    // Check description extraction
    assert_eq!(
        json["data"]["description"].as_str().unwrap(),
        "A simple test page for web scraping"
    );
}

#[tokio::test]
async fn test_scrape_url_validation() {
    let app = create_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": "not-a-valid-url"})),
    ).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_error(&json);
    assert_has_field(&json, "error");
}

// ============================================================================
// MARKDOWN CONVERSION TESTS
// ============================================================================

#[tokio::test]
async fn test_markdown_conversion_quality() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("article.html");

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/article", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["markdown"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);

    let markdown = json["data"]["markdown"].as_str().unwrap();

    // Check markdown quality
    assert!(markdown.contains("# Revolutionary Web Scraping Technology Announced"));
    assert!(markdown.contains("## Key Features"));
    assert!(markdown.contains("> \"This is a game changer"));
}

#[tokio::test]
async fn test_markdown_with_lists() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["markdown"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);

    let markdown = json["data"]["markdown"].as_str().unwrap();

    // Check list conversion
    assert!(markdown.contains("- List item 1") || markdown.contains("* List item 1"));
    assert!(markdown.contains("- List item 2") || markdown.contains("* List item 2"));
}

// ============================================================================
// METADATA EXTRACTION TESTS
// ============================================================================

#[tokio::test]
async fn test_metadata_extraction_basic() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_metadata_extracted(&json);

    let metadata = &json["data"]["metadata"];
    assert_eq!(metadata["title"].as_str().unwrap(), "Simple Test Page");
    assert_eq!(metadata["description"].as_str().unwrap(), "A simple test page for web scraping");
    assert_eq!(metadata["statusCode"].as_u64().unwrap(), 200);
}

#[tokio::test]
async fn test_metadata_open_graph_tags() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);

    let metadata = &json["data"]["metadata"];

    // Check Open Graph metadata
    assert_eq!(metadata["ogTitle"].as_str().unwrap(), "Simple Test Page");
    assert_eq!(metadata["ogDescription"].as_str().unwrap(), "Open Graph description for testing");
    assert_eq!(metadata["ogImage"].as_str().unwrap(), "https://example.com/image.jpg");
    assert_eq!(metadata["ogUrl"].as_str().unwrap(), "https://example.com/simple");
}

#[tokio::test]
async fn test_metadata_article_tags() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("article.html");

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/article", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);

    let metadata = &json["data"]["metadata"];

    // Check article metadata
    assert!(metadata.get("ogType").is_some());
    assert_eq!(metadata["ogType"].as_str().unwrap(), "article");
}

// ============================================================================
// LINK EXTRACTION TESTS
// ============================================================================

#[tokio::test]
async fn test_link_extraction() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["links"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_links_extracted(&json, 2);

    let links = json["data"]["links"].as_array().unwrap();
    assert!(links.iter().any(|l| l.as_str().unwrap().contains("page1")));
    assert!(links.iter().any(|l| l.as_str().unwrap().contains("page2")));
}

#[tokio::test]
async fn test_link_extraction_ecommerce() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("ecommerce.html");

    let mock = server
        .mock("GET", "/products")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/products", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["links"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_links_extracted(&json, 3); // At least 3 product links
}

// ============================================================================
// IMAGE EXTRACTION TESTS
// ============================================================================

#[tokio::test]
async fn test_image_extraction() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["images"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_images_extracted(&json, 2);

    let images = json["data"]["images"].as_array().unwrap();
    assert!(images.iter().any(|i| i.as_str().unwrap().contains("image1.jpg")));
    assert!(images.iter().any(|i| i.as_str().unwrap().contains("image2.png")));
}

#[tokio::test]
async fn test_image_extraction_with_og_image() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("article.html");

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/article", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["images"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_images_extracted(&json, 1);
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[tokio::test]
async fn test_error_invalid_url() {
    let app = create_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": "invalid-url"})),
    ).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_error(&json);
}

#[tokio::test]
async fn test_error_404_not_found() {
    let server = mock_server::create().await;

    let mock = server
        .mock("GET", "/notfound")
        .with_status(404)
        .with_body("Not Found")
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/notfound", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),
    ).await;

    mock.assert_async().await;
    // Should still return 200 but with error in response
    assert_eq!(status, StatusCode::OK);
    assert_error(&json);

    let metadata = &json["data"]["metadata"];
    assert_eq!(metadata["statusCode"].as_u64().unwrap(), 404);
}

#[tokio::test]
async fn test_error_timeout() {
    let server = mock_server::create().await;

    // This mock will delay response
    let mock = server
        .mock("GET", "/slow")
        .with_status(200)
        .with_body("Slow response")
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/slow", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "timeout": 100  // Very short timeout
        })),
    ).await;

    // Should handle timeout gracefully
    assert!(status == StatusCode::OK || status == StatusCode::REQUEST_TIMEOUT);
}

#[tokio::test]
async fn test_error_missing_url() {
    let app = create_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"formats": ["markdown"]})),
    ).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_error(&json);
}

#[tokio::test]
async fn test_error_malformed_json() {
    let app = create_app();

    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/v1/scrape")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{invalid json"))
        .unwrap();

    let response = tower::ServiceExt::oneshot(app, request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// ROBOTS.TXT TESTS
// ============================================================================

#[tokio::test]
async fn test_robots_txt_respect() {
    let server = mock_server::create().await;
    let robots = fixtures::load_robots_txt();

    let robots_mock = server
        .mock("GET", "/robots.txt")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body(&robots)
        .create_async()
        .await;

    let app = create_app();

    // Try to scrape a disallowed path
    let url = format!("{}/admin/secret", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),
    ).await;

    robots_mock.assert_async().await;

    // Should respect robots.txt and return error or warning
    assert!(status == StatusCode::OK || status == StatusCode::FORBIDDEN);
    if status == StatusCode::OK {
        // Check for warning about robots.txt
        assert!(json.get("warning").is_some() || json["success"].as_bool() == Some(false));
    }
}

#[tokio::test]
async fn test_robots_txt_allowed_path() {
    let server = mock_server::create().await;
    let robots = fixtures::load_robots_txt();
    let html = fixtures::load_html("simple.html");

    let robots_mock = server
        .mock("GET", "/robots.txt")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body(&robots)
        .create_async()
        .await;

    let html_mock = server
        .mock("GET", "/api/public/data")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/api/public/data", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),
    ).await;

    robots_mock.assert_async().await;
    html_mock.assert_async().await;

    // Should be allowed
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
}

// ============================================================================
// OUTPUT FORMAT TESTS
// ============================================================================

#[tokio::test]
async fn test_multiple_formats() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["markdown", "html", "links", "images"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);

    // All formats should be present
    assert_data_has_field(&json, "markdown");
    assert_data_has_field(&json, "html");
    assert_data_has_field(&json, "links");
    assert_data_has_field(&json, "images");
}

#[tokio::test]
async fn test_html_format() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["html"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_data_has_field(&json, "html");

    let html_content = json["data"]["html"].as_str().unwrap();
    assert!(!html_content.is_empty());
}

#[tokio::test]
async fn test_raw_html_format() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "formats": ["rawHtml"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_data_has_field(&json, "rawHtml");

    let raw_html = json["data"]["rawHtml"].as_str().unwrap();
    assert!(raw_html.contains("<!DOCTYPE html>"));
}

#[tokio::test]
async fn test_default_format_is_markdown() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({"url": url})),  // No formats specified
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_data_has_field(&json, "markdown");
}

// ============================================================================
// ADVANCED OPTIONS TESTS
// ============================================================================

#[tokio::test]
async fn test_only_main_content() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("with_ads.html");

    let mock = server
        .mock("GET", "/page")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/page", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "onlyMainContent": true
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);

    let markdown = json["data"]["markdown"].as_str().unwrap();
    // Should extract main content, not ads
    assert!(markdown.contains("Main Content"));
    assert!(markdown.contains("actual content"));
}

#[tokio::test]
async fn test_include_tags_filter() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("article.html");

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/article", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "includeTags": ["article"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);

    let markdown = json["data"]["markdown"].as_str().unwrap();
    assert!(markdown.contains("Revolutionary Web Scraping"));
}

#[tokio::test]
async fn test_exclude_tags_filter() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("article.html");

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/article", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "excludeTags": ["nav", "footer"]
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);

    let markdown = json["data"]["markdown"].as_str().unwrap();
    // Should not contain navigation or footer content
    assert!(!markdown.contains("Home") || !markdown.contains("Technology") || !markdown.contains("Business"));
}

#[tokio::test]
async fn test_custom_headers() {
    let server = mock_server::create().await;
    let html = fixtures::load_html("simple.html");

    let mock = server
        .mock("GET", "/simple")
        .match_header("user-agent", "CustomBot/1.0")
        .match_header("x-custom-header", "test-value")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(&html)
        .create_async()
        .await;

    let app = create_app();
    let url = format!("{}/simple", server.url());

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": url,
            "headers": {
                "User-Agent": "CustomBot/1.0",
                "X-Custom-Header": "test-value"
            }
        })),
    ).await;

    mock.assert_async().await;
    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
}

// ============================================================================
// REAL URL TESTS (Integration with test_urls.txt)
// ============================================================================

#[tokio::test]
#[ignore] // Ignore by default, run with --ignored for real URL tests
async fn test_real_url_example_com() {
    let app = create_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": "https://example.com",
            "formats": ["markdown", "links"]
        })),
    ).await;

    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_markdown_not_empty(&json);
    assert_metadata_extracted(&json);
}

#[tokio::test]
#[ignore]
async fn test_real_url_github_readme() {
    let app = create_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": "https://github.com/mendableai/firecrawl",
            "formats": ["markdown"]
        })),
    ).await;

    assert_eq!(status, StatusCode::OK);
    assert_success(&json);

    let markdown = json["data"]["markdown"].as_str().unwrap();
    // Should extract README content
    assert!(markdown.len() > 100);
}

#[tokio::test]
#[ignore]
async fn test_real_url_documentation_site() {
    let app = create_app();

    let (status, json) = make_request(
        app,
        "POST",
        "/api/v1/scrape",
        Some(json!({
            "url": "https://docs.python.org/3/",
            "formats": ["markdown", "links"]
        })),
    ).await;

    assert_eq!(status, StatusCode::OK);
    assert_success(&json);
    assert_links_extracted(&json, 5); // Should have multiple documentation links
}
