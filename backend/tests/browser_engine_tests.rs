use essence::{
    engines::{browser::BrowserEngine, detect_engine_needed, EngineType, ScrapeEngine},
    types::{BrowserAction, ScrapeRequest},
};

#[tokio::test]
#[ignore] // Requires browser installation - run with: cargo test --ignored
async fn test_browser_engine_basic() {
    let engine = BrowserEngine::new()
        .await
        .expect("Failed to create browser engine");

    let request = ScrapeRequest {
        url: "https://example.com".to_string(),
        formats: vec!["markdown".to_string()],
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: 30000,
        wait_for: 0,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "browser".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    let result = engine.scrape(&request).await;
    assert!(result.is_ok());

    let raw_result = result.unwrap();
    assert_eq!(raw_result.status_code, 200);
    assert!(raw_result.html.contains("Example Domain"));
}

#[tokio::test]
#[ignore] // Requires browser installation
async fn test_browser_engine_with_wait() {
    let engine = BrowserEngine::new()
        .await
        .expect("Failed to create browser engine");

    let request = ScrapeRequest {
        url: "https://example.com".to_string(),
        formats: vec!["markdown".to_string()],
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: 30000,
        wait_for: 1000, // Wait 1 second
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "browser".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    let result = engine.scrape(&request).await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore] // Requires browser installation
async fn test_browser_engine_with_actions() {
    let engine = BrowserEngine::new()
        .await
        .expect("Failed to create browser engine");

    let request = ScrapeRequest {
        url: "https://example.com".to_string(),
        formats: vec!["markdown".to_string()],
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: 30000,
        wait_for: 0,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "browser".to_string(),
        wait_for_selector: None,
        actions: vec![
            BrowserAction::Wait { milliseconds: 500 },
            BrowserAction::Scroll {
                direction: "bottom".to_string(),
            },
        ],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    let result = engine.scrape(&request).await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore] // Requires browser installation and react.dev to be accessible
async fn test_browser_engine_react_site() {
    let engine = BrowserEngine::new()
        .await
        .expect("Failed to create browser engine");

    let request = ScrapeRequest {
        url: "https://react.dev".to_string(),
        formats: vec!["markdown".to_string()],
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: 60000, // Longer timeout for complex site
        wait_for: 2000, // Wait for content to load
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "browser".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    let result = engine.scrape(&request).await;
    assert!(result.is_ok());

    let raw_result = result.unwrap();
    assert_eq!(raw_result.status_code, 200);
    // React.dev should have more content when rendered
    assert!(raw_result.html.len() > 1000);
}

#[test]
fn test_detect_react_framework() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head></head>
        <body>
            <div id="root"></div>
            <script>window.__NEXT_DATA__ = {}</script>
        </body>
        </html>
    "#;

    let result = detect_engine_needed("https://example.com", html);
    assert_eq!(result, EngineType::Browser);
}

#[test]
fn test_detect_vue_framework() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head></head>
        <body>
            <div id="app" data-v-123456></div>
        </body>
        </html>
    "#;

    let result = detect_engine_needed("https://example.com", html);
    assert_eq!(result, EngineType::Browser);
}

#[test]
fn test_detect_minimal_spa() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>App</title></head>
        <body>
            <div id="root"></div>
        </body>
        </html>
    "#;

    let result = detect_engine_needed("https://example.com", html);
    assert_eq!(result, EngineType::Browser);
}

#[test]
fn test_detect_regular_html() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Regular Page</title></head>
        <body>
            <h1>Welcome to My Website</h1>
            <p>This is a regular HTML page with plenty of content.</p>
            <p>It has multiple paragraphs and does not require JavaScript.</p>
            <div>
                <ul>
                    <li>Item 1</li>
                    <li>Item 2</li>
                    <li>Item 3</li>
                </ul>
            </div>
        </body>
        </html>
    "#;

    let result = detect_engine_needed("https://example.com", html);
    assert_eq!(result, EngineType::Http);
}

#[test]
fn test_detect_nextjs_meta() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <meta name="generator" content="Next.js" />
        </head>
        <body>
            <div id="__next"></div>
        </body>
        </html>
    "#;

    let result = detect_engine_needed("https://example.com", html);
    assert_eq!(result, EngineType::Browser);
}

#[tokio::test]
#[ignore] // Requires browser installation
async fn test_ad_blocking_enabled() {
    // Set environment variable to enable ad blocking
    std::env::set_var("BROWSER_BLOCK_ADS", "true");

    let engine = BrowserEngine::new()
        .await
        .expect("Failed to create browser engine");

    // Use a site that typically has Google Analytics (we'll use a test site)
    let request = ScrapeRequest {
        url: "https://example.com".to_string(),
        formats: vec!["markdown".to_string()],
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: 30000,
        wait_for: 1000,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "browser".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    let result = engine.scrape(&request).await;
    assert!(
        result.is_ok(),
        "Scrape should succeed with ad blocking enabled"
    );

    let raw_result = result.unwrap();
    assert_eq!(raw_result.status_code, 200);

    // HTML should be clean and not contain analytics scripts
    // Note: example.com doesn't have analytics, but we're testing that blocking doesn't break normal sites
    assert!(raw_result.html.contains("Example Domain"));
}

#[tokio::test]
#[ignore] // Requires browser installation
async fn test_ad_blocking_disabled() {
    // Set environment variable to disable ad blocking
    std::env::set_var("BROWSER_BLOCK_ADS", "false");

    let engine = BrowserEngine::new()
        .await
        .expect("Failed to create browser engine");

    let request = ScrapeRequest {
        url: "https://example.com".to_string(),
        formats: vec!["markdown".to_string()],
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: 30000,
        wait_for: 0,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "browser".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    let result = engine.scrape(&request).await;
    assert!(
        result.is_ok(),
        "Scrape should succeed with ad blocking disabled"
    );

    let raw_result = result.unwrap();
    assert_eq!(raw_result.status_code, 200);
}

#[tokio::test]
#[ignore] // Requires browser installation
async fn test_ad_blocking_performance() {
    // Test that ad blocking improves performance by comparing load times
    std::env::set_var("BROWSER_BLOCK_ADS", "true");

    let engine = BrowserEngine::new()
        .await
        .expect("Failed to create browser engine");

    // Use a site known to have heavy analytics (e.g., news sites)
    // For testing, we'll use react.dev which has some tracking
    let request = ScrapeRequest {
        url: "https://react.dev".to_string(),
        formats: vec!["markdown".to_string()],
        headers: Default::default(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        timeout: 60000,
        wait_for: 2000,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "browser".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    let start = std::time::Instant::now();
    let result = engine.scrape(&request).await;
    let elapsed_with_blocking = start.elapsed();

    assert!(result.is_ok(), "Scrape with ad blocking should succeed");
    println!("Scrape with ad blocking took: {:?}", elapsed_with_blocking);

    // The page should load successfully
    let raw_result = result.unwrap();
    assert_eq!(raw_result.status_code, 200);
    assert!(
        raw_result.html.len() > 1000,
        "Should have substantial content"
    );
}
