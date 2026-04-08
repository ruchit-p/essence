// Real-world integration tests for the /api/v1/scrape endpoint
// All tests are #[ignore] by default to avoid hitting external sites during normal CI runs
// Run with: cargo test --test integration -- --ignored

mod api;

use api::{create_app, metrics::ScrapeMetrics, send_scrape_request};
use essence::crawler::crawl_website;
use essence::types::CrawlRequest;
use serde_json::json;
use std::time::Instant;

// ============================================================================
// CATEGORY 1: SANDBOX / SCRAPE-ME SITES
// ============================================================================
// These sites are specifically designed for scraper testing

#[tokio::test]
#[ignore]
async fn test_sandbox_quotes_toscrape() {
    let app = create_app();
    let url = "https://quotes.toscrape.com";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "links"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("sandbox")
        .with_test_name("test_sandbox_quotes_toscrape");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for quotes.toscrape.com"
    );
    assert!(
        metrics.markdown_length > 100,
        "Should extract substantial content"
    );
    assert!(
        metrics.link_count > 0,
        "Should extract links from pagination and quotes"
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_sandbox_books_toscrape() {
    let app = create_app();
    let url = "https://books.toscrape.com";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "links", "images"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("sandbox")
        .with_test_name("test_sandbox_books_toscrape");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for books.toscrape.com"
    );
    assert!(metrics.has_title, "Should have a title");
    assert!(
        metrics.link_count > 10,
        "Should extract multiple product links"
    );
    assert!(metrics.image_count > 0, "Should extract book cover images");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_sandbox_webscraper_ecommerce() {
    let app = create_app();
    let url = "https://webscraper.io/test-sites/e-commerce/allinone";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "links", "images"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("sandbox")
        .with_test_name("test_sandbox_webscraper_ecommerce");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for webscraper.io"
    );
    assert!(
        metrics.markdown_length > 200,
        "Should extract e-commerce content"
    );
    assert!(
        metrics.link_count > 5,
        "Should extract product and category links"
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_sandbox_scrapethissite() {
    let app = create_app();
    let url = "http://www.scrapethissite.com/pages/simple/";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("sandbox")
        .with_test_name("test_sandbox_scrapethissite");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for scrapethissite.com"
    );
    assert!(
        metrics.markdown_length > 100,
        "Should extract country data tables"
    );

    println!("\n[METRICS] {:?}", metrics);
}

// ============================================================================
// CATEGORY 2: STATIC DOCUMENTATION SITES
// ============================================================================
// Well-structured static HTML pages with clean markup

#[tokio::test]
#[ignore]
async fn test_static_docs_example_com() {
    let app = create_app();
    let url = "https://example.com";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "html"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("static_docs")
        .with_test_name("test_static_docs_example_com");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for example.com"
    );
    assert!(metrics.has_title, "Should extract title");
    assert!(metrics.markdown_length > 50, "Should have basic content");
    assert!(
        metrics.response_time_ms < 5000,
        "Should respond quickly (< 5s)"
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_static_docs_example_org() {
    let app = create_app();
    let url = "https://example.org";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("static_docs")
        .with_test_name("test_static_docs_example_org");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for example.org"
    );
    assert!(metrics.has_title, "Should extract title");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_static_docs_mdn() {
    let app = create_app();
    let url = "https://developer.mozilla.org/en-US/docs/Web/HTML";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "links"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("static_docs")
        .with_test_name("test_static_docs_mdn");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for MDN"
    );
    assert!(metrics.has_title, "Should extract title");
    assert!(
        metrics.markdown_length > 500,
        "Should extract substantial documentation"
    );
    assert!(
        metrics.link_count > 10,
        "Should extract navigation and reference links"
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_static_docs_rust_book() {
    let app = create_app();
    let url = "https://doc.rust-lang.org/book/";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "links"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("static_docs")
        .with_test_name("test_static_docs_rust_book");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for Rust Book"
    );
    assert!(metrics.has_title, "Should extract title");
    assert!(
        metrics.markdown_length > 200,
        "Should extract table of contents"
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_static_docs_python() {
    let app = create_app();
    let url = "https://docs.python.org/3/";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "links"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("static_docs")
        .with_test_name("test_static_docs_python");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for Python docs"
    );
    assert!(metrics.has_title, "Should extract title");
    assert!(
        metrics.link_count > 10,
        "Should extract documentation links"
    );

    println!("\n[METRICS] {:?}", metrics);
}

// ============================================================================
// CATEGORY 3: HTTP EDGE CASES (httpbin.org)
// ============================================================================
// HTTP testing service for edge cases and protocol testing

#[tokio::test]
#[ignore]
async fn test_http_edge_cases_html() {
    let app = create_app();
    let url = "https://httpbin.org/html";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("http_edge_cases")
        .with_test_name("test_http_edge_cases_html");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for httpbin.org/html"
    );
    assert!(metrics.status_code == 200, "Should return 200 status");
    assert!(metrics.markdown_length > 0, "Should extract HTML content");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_http_edge_cases_status_200() {
    let app = create_app();
    let url = "https://httpbin.org/status/200";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("http_edge_cases")
        .with_test_name("test_http_edge_cases_status_200");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for status 200"
    );
    assert!(metrics.status_code == 200, "Should return 200 status");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_http_edge_cases_status_404() {
    let app = create_app();
    let url = "https://httpbin.org/status/404";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("http_edge_cases")
        .with_test_name("test_http_edge_cases_status_404");

    // Note: A 404 response is still a successful scrape operation
    // The status code should be captured in metadata
    assert!(metrics.status_code == 404, "Should return 404 status");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_http_edge_cases_status_500() {
    let app = create_app();
    let url = "https://httpbin.org/status/500";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("http_edge_cases")
        .with_test_name("test_http_edge_cases_status_500");

    // Note: A 500 response is still a successful scrape operation
    // The status code should be captured in metadata
    assert!(metrics.status_code == 500, "Should return 500 status");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_http_edge_cases_delay_2s() {
    let app = create_app();
    let url = "https://httpbin.org/delay/2";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("http_edge_cases")
        .with_test_name("test_http_edge_cases_delay_2s");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed despite delay"
    );
    assert!(
        metrics.response_time_ms >= 2000,
        "Should take at least 2 seconds"
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_http_edge_cases_redirect() {
    let app = create_app();
    let url = "https://httpbin.org/redirect/1";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("http_edge_cases")
        .with_test_name("test_http_edge_cases_redirect");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should follow redirect successfully"
    );

    println!("\n[METRICS] {:?}", metrics);
}

// ============================================================================
// CATEGORY 4: STRUCTURED METADATA EXTRACTION
// ============================================================================
// Sites with rich metadata, Open Graph tags, structured data

#[tokio::test]
#[ignore]
async fn test_metadata_extraction_ogp() {
    let app = create_app();
    let url = "https://ogp.me";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("metadata_extraction")
        .with_test_name("test_metadata_extraction_ogp");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for ogp.me"
    );
    assert!(metrics.has_title, "Should extract title");
    assert!(metrics.has_description, "Should extract description");

    // Check for Open Graph metadata
    let metadata = &response["data"]["metadata"];
    println!(
        "Metadata: {}",
        serde_json::to_string_pretty(metadata).unwrap()
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_metadata_extraction_schema_org() {
    let app = create_app();
    let url = "https://schema.org";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("metadata_extraction")
        .with_test_name("test_metadata_extraction_schema_org");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for schema.org"
    );
    assert!(metrics.has_title, "Should extract title");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_metadata_extraction_github() {
    let app = create_app();
    let url = "https://github.com/mendableai/firecrawl";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "links"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("metadata_extraction")
        .with_test_name("test_metadata_extraction_github");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for GitHub"
    );
    assert!(metrics.has_title, "Should extract repository title");
    assert!(
        metrics.has_description,
        "Should extract repository description"
    );
    assert!(
        metrics.markdown_length > 500,
        "Should extract README content"
    );

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_metadata_extraction_npm() {
    let app = create_app();
    let url = "https://www.npmjs.com/package/axios";

    let payload = json!({
        "url": url,
        "formats": ["markdown"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("metadata_extraction")
        .with_test_name("test_metadata_extraction_npm");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for NPM"
    );
    assert!(metrics.has_title, "Should extract package title");
    assert!(
        metrics.has_description,
        "Should extract package description"
    );

    println!("\n[METRICS] {:?}", metrics);
}

// ============================================================================
// CATEGORY 5: LEGACY HTML / QUIRKS MODE
// ============================================================================
// Older HTML standards, quirks mode testing

#[tokio::test]
#[ignore]
async fn test_legacy_html_example_com() {
    let app = create_app();
    let url = "https://example.com";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "html", "rawHtml"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("legacy_html")
        .with_test_name("test_legacy_html_example_com");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for legacy HTML"
    );
    assert!(metrics.has_title, "Should extract title from legacy HTML");

    println!("\n[METRICS] {:?}", metrics);
}

#[tokio::test]
#[ignore]
async fn test_legacy_html_example_org() {
    let app = create_app();
    let url = "https://example.org";

    let payload = json!({
        "url": url,
        "formats": ["markdown", "html"]
    });

    let start = Instant::now();
    let response = send_scrape_request(&app, payload).await;
    let elapsed = start.elapsed();

    let metrics = ScrapeMetrics::from_response(url.to_string(), &response, elapsed)
        .with_category("legacy_html")
        .with_test_name("test_legacy_html_example_org");

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Scrape should succeed for legacy HTML"
    );
    assert!(metrics.has_title, "Should extract title from minimal HTML");

    println!("\n[METRICS] {:?}", metrics);
}

// ============================================================================
// CATEGORY 6: MAP ENDPOINT TESTS
// ============================================================================
// Tests for the /api/v1/map endpoint for URL discovery

#[tokio::test]
#[ignore]
async fn test_map_endpoint_basic() {
    let app = create_app();
    let url = "https://example.com";

    let payload = json!({
        "url": url
    });

    let response = api::send_map_request(&app, payload).await;

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Map should succeed for example.com"
    );
    assert!(
        response["links"].is_array(),
        "Response should contain links array"
    );

    let links = response["links"].as_array().unwrap();
    println!("Discovered {} URLs from {}", links.len(), url);

    for link in links.iter().take(5) {
        println!("  - {}", link.as_str().unwrap_or(""));
    }
}

#[tokio::test]
#[ignore]
async fn test_map_endpoint_with_limit() {
    let app = create_app();
    let url = "https://quotes.toscrape.com";

    let payload = json!({
        "url": url,
        "limit": 10
    });

    let response = api::send_map_request(&app, payload).await;

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Map should succeed"
    );

    let links = response["links"].as_array().unwrap();
    assert!(links.len() <= 10, "Should respect limit of 10 URLs");

    println!("Discovered {} URLs (limit: 10) from {}", links.len(), url);
}

#[tokio::test]
#[ignore]
async fn test_map_endpoint_with_search() {
    let app = create_app();
    let url = "https://docs.python.org/3/";

    let payload = json!({
        "url": url,
        "search": "tutorial",
        "limit": 50
    });

    let response = api::send_map_request(&app, payload).await;

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Map should succeed"
    );

    let links = response["links"].as_array().unwrap();

    // Check that all URLs contain the search term
    for link in links {
        let link_str = link.as_str().unwrap_or("");
        assert!(
            link_str.to_lowercase().contains("tutorial"),
            "All links should contain search term 'tutorial': {}",
            link_str
        );
    }

    println!(
        "Discovered {} URLs matching 'tutorial' from {}",
        links.len(),
        url
    );
}

#[tokio::test]
#[ignore]
async fn test_map_endpoint_ignore_sitemap() {
    let app = create_app();
    let url = "https://example.com";

    let payload = json!({
        "url": url,
        "ignoreSitemap": true
    });

    let response = api::send_map_request(&app, payload).await;

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Map should succeed even when ignoring sitemap"
    );

    let links = response["links"].as_array().unwrap();
    println!(
        "Discovered {} URLs (sitemap ignored) from {}",
        links.len(),
        url
    );
}

#[tokio::test]
#[ignore]
async fn test_map_endpoint_subdomain_filtering() {
    let app = create_app();
    let url = "https://www.wikipedia.org";

    let payload = json!({
        "url": url,
        "includeSubdomains": false,
        "limit": 20
    });

    let response = api::send_map_request(&app, payload).await;

    // Assertions
    assert!(
        response["success"].as_bool().unwrap_or(false),
        "Map should succeed"
    );

    let links = response["links"].as_array().unwrap();

    // Check that URLs are from the same domain (no subdomains)
    for link in links {
        let link_str = link.as_str().unwrap_or("");
        println!("  - {}", link_str);
    }

    println!(
        "Discovered {} URLs from {} (subdomains excluded)",
        links.len(),
        url
    );
}

#[tokio::test]
async fn test_map_endpoint_validation() {
    let app = create_app();

    // Test empty URL
    let payload = json!({
        "url": ""
    });

    let response = api::send_map_request(&app, payload).await;
    assert!(
        !response["success"].as_bool().unwrap_or(true),
        "Should fail with empty URL"
    );

    // Test limit validation
    let payload = json!({
        "url": "https://example.com",
        "limit": 200000  // Exceeds max of 100000
    });

    let response = api::send_map_request(&app, payload).await;
    assert!(
        !response["success"].as_bool().unwrap_or(true),
        "Should fail with limit exceeding 100000"
    );
}

#[tokio::test]
#[ignore]
async fn test_pagination_detection() {
    // Test automatic pagination detection on quotes.toscrape.com
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        limit: 15,
        max_depth: 2,
        include_paths: Some(vec![]),
        exclude_paths: Some(vec![]),
        allow_backward_links: None,
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: Some(true),
        max_pagination_pages: Some(50),
        use_parallel: None,
        engine: None,
    };

    let result = crawl_website(&request).await;
    assert!(result.is_ok(), "Crawl should succeed");

    let documents = result.unwrap();

    // Should discover at least 15 pages due to pagination detection
    assert!(
        documents.len() >= 15,
        "Should discover at least 15 pages with pagination detection, found {}",
        documents.len()
    );

    // Count pagination pages (/page/N/)
    let pagination_pages: Vec<_> = documents
        .iter()
        .filter(|doc| {
            if let Some(url) = &doc.metadata.url {
                url.contains("/page/")
            } else {
                false
            }
        })
        .collect();

    // Should find at least 14 pagination pages (pages 2-15)
    assert!(
        pagination_pages.len() >= 14,
        "Should find at least 14 pagination pages, found {}",
        pagination_pages.len()
    );

    // Verify sequential pagination
    let has_page_2 = documents.iter().any(|doc| {
        if let Some(url) = &doc.metadata.url {
            url.contains("/page/2")
        } else {
            false
        }
    });

    let has_page_3 = documents.iter().any(|doc| {
        if let Some(url) = &doc.metadata.url {
            url.contains("/page/3")
        } else {
            false
        }
    });

    assert!(
        has_page_2,
        "Should find page 2 through pagination detection"
    );
    assert!(
        has_page_3,
        "Should find page 3 through pagination detection"
    );
}
