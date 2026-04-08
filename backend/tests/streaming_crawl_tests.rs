use essence::crawler::crawl_website_stream;
use essence::types::{CrawlEvent, CrawlRequest};
use tokio::sync::mpsc;

#[tokio::test]
#[ignore] // Requires network access
async fn test_streaming_crawl_basic() {
    // Create a channel to receive events
    let (tx, mut rx) = mpsc::channel(100);

    // Create a simple crawl request
    let request = CrawlRequest {
        url: "https://example.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 1,
        limit: 5,
        allow_backward_links: None,
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
        engine: None,
    };

    // Spawn crawl task
    let crawl_handle = tokio::spawn(async move { crawl_website_stream(request, tx).await });

    // Collect events
    let mut events = Vec::new();
    let mut document_count = 0;
    let mut status_count = 0;
    let mut complete_count = 0;

    while let Some(event_result) = rx.recv().await {
        match event_result {
            Ok(event) => {
                match &event {
                    CrawlEvent::Status { .. } => status_count += 1,
                    CrawlEvent::Document { .. } => document_count += 1,
                    CrawlEvent::Complete {
                        total_pages,
                        success,
                        errors,
                    } => {
                        complete_count += 1;
                        println!(
                            "Crawl completed: {} total, {} success, {} errors",
                            total_pages, success, errors
                        );
                    }
                    CrawlEvent::Error { url, error } => {
                        println!("Error crawling {}: {}", url, error);
                    }
                }
                events.push(event);
            }
            Err(e) => {
                panic!("Received error event: {}", e);
            }
        }
    }

    // Wait for crawl to complete
    let result = crawl_handle.await.unwrap();
    assert!(result.is_ok(), "Crawl should complete successfully");

    // Verify we got events
    assert!(status_count > 0, "Should receive status events");
    assert!(document_count > 0, "Should receive document events");
    assert_eq!(
        complete_count, 1,
        "Should receive exactly one complete event"
    );
}

#[tokio::test]
async fn test_streaming_crawl_invalid_url() {
    let (tx, _rx) = mpsc::channel(100);

    let request = CrawlRequest {
        url: "not-a-valid-url".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 1,
        limit: 5,
        allow_backward_links: None,
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
        engine: None,
    };

    let result = crawl_website_stream(request, tx).await;

    // Should fail with invalid URL
    assert!(result.is_err(), "Should fail with invalid URL");
}

#[tokio::test]
#[ignore] // Requires network access
async fn test_streaming_crawl_with_limit() {
    let (tx, mut rx) = mpsc::channel(100);

    let limit = 3;
    let request = CrawlRequest {
        url: "https://example.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 2,
        limit,
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
        engine: None,
    };

    tokio::spawn(async move {
        let _ = crawl_website_stream(request, tx).await;
    });

    let mut document_count = 0;

    while let Some(event_result) = rx.recv().await {
        if let Ok(CrawlEvent::Document { .. }) = event_result {
            document_count += 1;
        }
    }

    // Should not exceed the limit
    assert!(
        document_count <= limit as usize,
        "Document count {} should not exceed limit {}",
        document_count,
        limit
    );
}

#[tokio::test]
#[ignore] // Requires network access
async fn test_streaming_crawl_client_disconnect() {
    let (tx, rx) = mpsc::channel(100);

    let request = CrawlRequest {
        url: "https://example.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 3,
        limit: 100, // Large limit
        allow_backward_links: Some(true),
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
        engine: None,
    };

    let crawl_handle = tokio::spawn(async move { crawl_website_stream(request, tx).await });

    // Drop receiver immediately to simulate client disconnect
    drop(rx);

    // Crawl should handle disconnection gracefully
    let result = crawl_handle.await.unwrap();
    assert!(
        result.is_ok(),
        "Crawl should handle client disconnect gracefully"
    );
}

#[tokio::test]
#[ignore] // Requires network access
async fn test_streaming_event_order() {
    let (tx, mut rx) = mpsc::channel(100);

    let request = CrawlRequest {
        url: "https://example.com".to_string(),
        exclude_paths: None,
        include_paths: None,
        max_depth: 1,
        limit: 3,
        allow_backward_links: None,
        allow_external_links: None,
        ignore_sitemap: None,
        detect_pagination: None,
        max_pagination_pages: None,
        use_parallel: None,
        engine: None,
    };

    tokio::spawn(async move {
        let _ = crawl_website_stream(request, tx).await;
    });

    let mut events = Vec::new();
    while let Some(event_result) = rx.recv().await {
        if let Ok(event) = event_result {
            events.push(event);
        }
    }

    // Last event should be Complete
    if let Some(last_event) = events.last() {
        assert!(
            matches!(last_event, CrawlEvent::Complete { .. }),
            "Last event should be Complete"
        );
    } else {
        panic!("No events received");
    }
}
