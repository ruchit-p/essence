use essence::types::{CrawlRequest, CrawlResponse};
use reqwest::StatusCode;

const SERVER_URL: &str = "http://localhost:8080";

/// Helper to make a crawl request
async fn crawl_request(request: &CrawlRequest) -> (StatusCode, CrawlResponse) {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/v1/crawl", SERVER_URL))
        .json(request)
        .send()
        .await
        .expect("Failed to send request");

    let status = response.status();
    let body = response
        .json::<CrawlResponse>()
        .await
        .expect("Failed to parse response");

    (status, body)
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_single_page() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 0, // Only crawl the starting page
        limit: 1,
        allow_backward_links: None,
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();
    assert_eq!(documents.len(), 1);
    assert!(documents[0].url.is_some());
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_with_depth() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 1,
        limit: 10,
        allow_backward_links: Some(true), // Allow entire domain
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();
    assert!(documents.len() > 1); // Should crawl multiple pages
    assert!(documents.len() <= 10); // Should respect limit
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_with_limit() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 2,
        limit: 3, // Strict limit
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();
    assert!(documents.len() <= 3); // Should not exceed limit
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_with_exclude_paths() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: Some(vec!["/tag/*".to_string()]), // Exclude tag pages
        include_paths: None,
        max_depth: 2,
        limit: 20,
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();
    // Verify no tag pages in results
    for doc in documents {
        if let Some(url) = &doc.url {
            assert!(!url.contains("/tag/"));
        }
    }
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_with_include_paths() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: None,
        include_paths: Some(vec!["/page/*".to_string()]), // Only page URLs
        max_depth: 2,
        limit: 10,
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();
    // At minimum should have crawled the base page
    assert!(!documents.is_empty());
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_no_external_links() {
    let request = CrawlRequest {
        url: "https://example.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 1,
        limit: 10,
        allow_backward_links: Some(true),
        allow_external_links: Some(false), // Block external links
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();
    // Verify all URLs are from the same domain
    for doc in documents {
        if let Some(url) = &doc.url {
            assert!(url.contains("example.com"));
        }
    }
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_invalid_url() {
    let request = CrawlRequest {
        url: "not-a-valid-url".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 2,
        limit: 100,
        allow_backward_links: None,
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!response.success);
    assert!(response.error.is_some());
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_empty_url() {
    let request = CrawlRequest {
        url: "".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 2,
        limit: 100,
        allow_backward_links: None,
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!response.success);
    assert!(response.error.is_some());
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_deduplication() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 2,
        limit: 100,
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();

    // Check for duplicate URLs
    let mut urls = std::collections::HashSet::new();
    for doc in &documents {
        if let Some(url) = &doc.url {
            assert!(!urls.contains(url), "Found duplicate URL: {}", url);
            urls.insert(url.clone());
        }
    }
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_depth_limiting() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 0, // Only base page
        limit: 100,
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();
    assert_eq!(documents.len(), 1); // Should only crawl the base page
}

#[tokio::test]
#[ignore = "requires running server"]
async fn test_crawl_returns_documents_with_content() {
    let request = CrawlRequest {
        url: "https://quotes.toscrape.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 1,
        limit: 5,
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
    };

    let (status, response) = crawl_request(&request).await;

    assert_eq!(status, StatusCode::OK);
    assert!(response.success);
    assert!(response.data.is_some());

    let documents = response.data.unwrap();

    // Verify each document has content
    for doc in documents {
        assert!(doc.url.is_some());
        assert!(doc.markdown.is_some());
        assert!(doc.metadata.status_code > 0);
    }
}
