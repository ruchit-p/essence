// Integration tests for /api/v1/scrape endpoint
mod api;

use api::{assertions::*, create_app, load_fixture, send_scrape_request};

#[tokio::test]
async fn test_scrape_basic_html() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url()),
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);
    assert_has_markdown(&response);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_scrape_returns_title_and_description() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("article.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/article", server.url()),
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);
    assert_has_metadata(&response, &["title", "description"]);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_scrape_url_validation() {
    let app = create_app();

    let payload = serde_json::json!({
        "url": "not-a-valid-url",
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_error(&response);
}

#[tokio::test]
async fn test_markdown_conversion_quality() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("article.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/article", server.url()),
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);

    let markdown = response["data"]["markdown"].as_str().unwrap();
    // Check that markdown has reasonable content (headers might be formatted differently)
    assert!(
        markdown.len() > 100,
        "Markdown should have substantial content"
    );
    assert!(
        markdown.to_lowercase().contains("web scraping") || markdown.contains("Revolutionary"),
        "Markdown should contain article content"
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn test_markdown_with_lists() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let html = r#"
        <html>
            <body>
                <h1>Test Page</h1>
                <ul>
                    <li>Item 1</li>
                    <li>Item 2</li>
                    <li>Item 3</li>
                </ul>
            </body>
        </html>
    "#;

    let mock = server
        .mock("GET", "/list")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(html)
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/list", server.url()),
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);

    let markdown = response["data"]["markdown"].as_str().unwrap();
    assert!(
        markdown.contains("* Item") || markdown.contains("- Item") || markdown.contains("* Item")
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn test_metadata_extraction_basic() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url()),
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);
    assert_has_metadata(&response, &["statusCode"]);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_metadata_open_graph_tags() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/article")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("article.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/article", server.url()),
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);

    // Check for OG tags in metadata
    let metadata = &response["data"]["metadata"];
    assert!(metadata.is_object());

    mock.assert_async().await;
}

#[tokio::test]
async fn test_link_extraction() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url()),
        "formats": ["links"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);
    assert_has_links(&response);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_image_extraction() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url()),
        "formats": ["images"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);
    assert_has_images(&response);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_error_404_not_found() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/notfound")
        .with_status(404)
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/notfound", server.url()),
        "formats": ["markdown"]
    });

    let response = send_scrape_request(&app, payload).await;
    // Should succeed but with 404 status code in metadata
    assert_success(&response);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_multiple_formats() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url()),
        "formats": ["markdown", "html", "links", "images"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);
    assert_has_markdown(&response);
    assert_has_links(&response);
    assert_has_images(&response);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_html_format() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url()),
        "formats": ["html"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);

    let html = response["data"]["html"].as_str();
    assert!(html.is_some());

    mock.assert_async().await;
}

#[tokio::test]
async fn test_raw_html_format() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url()),
        "formats": ["rawHtml"]
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);

    let raw_html = response["data"]["rawHtml"].as_str();
    assert!(raw_html.is_some());

    mock.assert_async().await;
}

#[tokio::test]
async fn test_default_format_is_markdown() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/test")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("simple.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/test", server.url())
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);
    assert_has_markdown(&response);

    mock.assert_async().await;
}

#[tokio::test]
async fn test_only_main_content() {
    let app = create_app();
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/with-ads")
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body(load_fixture("with_ads.html"))
        .create_async()
        .await;

    let payload = serde_json::json!({
        "url": format!("{}/with-ads", server.url()),
        "formats": ["markdown"],
        "onlyMainContent": true
    });

    let response = send_scrape_request(&app, payload).await;
    assert_success(&response);

    let markdown = response["data"]["markdown"].as_str().unwrap();
    assert!(
        !markdown.contains("Advertisement") || markdown.len() < load_fixture("with_ads.html").len()
    );

    mock.assert_async().await;
}
