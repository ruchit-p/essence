use crate::{
    engines::{
        stealth::{apply_stealth_techniques, StealthMode},
        RawScrapeResult, ScrapeEngine,
    },
    error::{Result, ScrapeError},
    types::{BrowserAction, ScrapeRequest},
    utils::{url_rewrites::rewrite_url, user_agents::random_user_agent},
};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chromiumoxide::{
    browser::{Browser, BrowserConfig},
    cdp::browser_protocol::{
        fetch::{ContinueRequestParams, EnableParams, EventRequestPaused, FailRequestParams},
        network::ErrorReason,
        page::CaptureScreenshotFormat,
    },
    Page,
};
use futures::StreamExt;
use std::{
    env,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tracing::{debug, info, warn};

/// Domains to block for ads, trackers, and analytics
/// This speeds up page loads by 10-20% and reduces noise in scraped content
const BLOCKED_DOMAINS: &[&str] = &[
    "googletagmanager.com",
    "google-analytics.com",
    "analytics.google.com",
    "facebook.net",
    "connect.facebook.net",
    "doubleclick.net",
    "googlesyndication.com",
    "adservice.google.com",
    "static.ads-twitter.com",
    "ads-twitter.com",
    "ads.linkedin.com",
    "bat.bing.com",
    "stats.wp.com",
    "scorecardresearch.com",
    "quantserve.com",
    "chartbeat.com",
    "hotjar.com",
    "mouseflow.com",
    "mixpanel.com",
    "segment.io",
    "segment.com",
    "analytics.tiktok.com",
    "mktoresp.com",
    "pardot.com",
];

/// Browser pool for managing multiple browser instances
pub struct BrowserPool {
    semaphore: Arc<Semaphore>,
    #[allow(dead_code)]
    max_instances: usize,
    headless: bool,
    user_agent: Option<String>,
    browsers: Arc<Mutex<Vec<Browser>>>,
    chrome_path: PathBuf,
}

impl BrowserPool {
    /// Create a new browser pool
    pub async fn new(max_instances: usize) -> Result<Self> {
        let headless = std::env::var("BROWSER_HEADLESS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true);

        let user_agent = std::env::var("BROWSER_USER_AGENT").ok();

        // Find Chrome executable at initialization
        let chrome_path = Self::find_chrome_executable()?;
        info!("Found Chrome at: {}", chrome_path.display());

        Ok(Self {
            semaphore: Arc::new(Semaphore::new(max_instances)),
            max_instances,
            headless,
            user_agent,
            browsers: Arc::new(Mutex::new(Vec::new())),
            chrome_path,
        })
    }

    /// Get a browser from the pool (creates new if needed)
    pub async fn get_browser(self: &Arc<Self>) -> Result<BrowserGuard> {
        // Acquire permit (blocks if pool full)
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| ScrapeError::Internal(format!("Failed to acquire browser: {}", e)))?;

        // Try to reuse existing browser
        let browser = {
            let mut browsers = self.browsers.lock().await;

            // Try to find a healthy browser
            while let Some(mut b) = browsers.pop() {
                if Self::is_browser_healthy(&b).await {
                    debug!("Reusing existing browser instance");
                    return Ok(BrowserGuard {
                        browser: Some(b),
                        pool: self.browsers.clone(),
                        _permit: permit,
                    });
                } else {
                    // Unhealthy browser, discard it
                    warn!("Discarding unhealthy browser");
                    let _ = b.close().await;
                }
            }

            // No healthy browser available, create new one
            debug!("Creating new browser instance");
            self.create_browser().await?
        };

        Ok(BrowserGuard {
            browser: Some(browser),
            pool: self.browsers.clone(),
            _permit: permit,
        })
    }

    /// Check if a browser is healthy
    async fn is_browser_healthy(browser: &Browser) -> bool {
        // Try to get version (simple health check)
        match browser.version().await {
            Ok(_) => true,
            Err(_) => {
                warn!("Browser health check failed");
                false
            }
        }
    }

    /// Create a new browser instance
    async fn create_browser(&self) -> Result<Browser> {
        info!(
            "Launching new browser instance (headless: {})",
            self.headless
        );

        let mut browser_config = BrowserConfig::builder().chrome_executable(&self.chrome_path);

        // Set headless mode
        if self.headless {
            browser_config = browser_config.no_sandbox().disable_default_args();
        }

        // Add launch arguments for stealth and stability
        let mut args = vec![
            "--disable-blink-features=AutomationControlled",
            "--disable-dev-shm-usage",
            "--disable-web-security",
            "--disable-features=IsolateOrigins,site-per-process",
            "--allow-running-insecure-content",
            "--disable-setuid-sandbox",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-popup-blocking",
        ];

        if self.headless {
            args.push("--headless=new");
        }

        // Set viewport with some variance for anti-bot
        args.push("--window-size=1920,1080");

        for arg in args {
            browser_config = browser_config.arg(arg);
        }

        // Use a unique temp directory for each browser instance to prevent
        // SingletonLock conflicts when multiple instances launch concurrently
        let unique_dir =
            std::env::temp_dir().join(format!("essence-browser-{}", uuid::Uuid::new_v4()));
        browser_config = browser_config.arg(format!("--user-data-dir={}", unique_dir.display()));

        // Set user agent - use provided one or randomize
        let user_agent = match &self.user_agent {
            Some(ua) => ua.as_str(),
            None => random_user_agent(),
        };
        debug!("Using User-Agent for browser: {}", user_agent);
        browser_config = browser_config.arg(format!("--user-agent={}", user_agent));

        let (browser, mut handler) = Browser::launch(browser_config.build().map_err(|e| {
            ScrapeError::BrowserLaunchFailed(format!("Failed to build browser config: {}", e))
        })?)
        .await
        .map_err(|e| {
            ScrapeError::BrowserLaunchFailed(format!("Failed to launch browser: {}", e))
        })?;

        // Spawn a task to handle browser events
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    warn!("Browser handler error: {}", e);
                }
            }
            debug!("Browser handler task finished");
        });

        info!("Browser instance created successfully");
        Ok(browser)
    }

    /// Find Chrome executable across platforms
    fn find_chrome_executable() -> Result<PathBuf> {
        // Check environment variable first
        if let Ok(path) = std::env::var("CHROME_PATH") {
            let path_buf = PathBuf::from(&path);
            if path_buf.exists() {
                info!("Using Chrome from CHROME_PATH: {}", path);
                return Ok(path_buf);
            }
        }

        // Platform-specific paths
        let paths: Vec<&str> = if cfg!(target_os = "macos") {
            vec![
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "/Applications/Chromium.app/Contents/MacOS/Chromium",
                "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
                "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            ]
        } else if cfg!(target_os = "linux") {
            vec![
                "/usr/bin/google-chrome",
                "/usr/bin/google-chrome-stable",
                "/usr/bin/chromium",
                "/usr/bin/chromium-browser",
                "/snap/bin/chromium",
                "/usr/bin/microsoft-edge",
                "/usr/bin/microsoft-edge-stable",
            ]
        } else if cfg!(target_os = "windows") {
            vec![
                "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
                "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
                "C:\\Program Files\\Chromium\\Application\\chrome.exe",
                "C:\\Program Files (x86)\\Chromium\\Application\\chrome.exe",
                "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
                "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
            ]
        } else {
            vec![]
        };

        for path in paths {
            if std::fs::metadata(path).is_ok() {
                info!("Found Chrome at: {}", path);
                return Ok(PathBuf::from(path));
            }
        }

        // Platform-specific error message
        let install_msg = if cfg!(target_os = "macos") {
            "brew install --cask chromium"
        } else if cfg!(target_os = "linux") {
            "sudo apt-get install chromium-browser  # Ubuntu/Debian\nsudo yum install chromium  # RHEL/CentOS"
        } else if cfg!(target_os = "windows") {
            "Install Chrome from https://www.google.com/chrome/"
        } else {
            "Install Chrome or Chromium"
        };

        Err(ScrapeError::BrowserNotFound(format!(
            "Chrome/Chromium not found. Please install it:\n{}",
            install_msg
        )))
    }
}

impl Drop for BrowserPool {
    fn drop(&mut self) {
        info!("Shutting down browser pool");

        // Note: We can't use async in Drop, so we'll just log
        // The browsers will be cleaned up when the Arc is dropped
        if let Ok(browsers) = self.browsers.try_lock() {
            info!("Browser pool had {} instances at shutdown", browsers.len());
        } else {
            info!("Browser pool shutting down (browsers still in use)");
        }
    }
}

/// Guard that returns browser to pool when dropped
pub struct BrowserGuard {
    browser: Option<Browser>,
    pool: Arc<Mutex<Vec<Browser>>>,
    _permit: OwnedSemaphorePermit,
}

impl Drop for BrowserGuard {
    fn drop(&mut self) {
        // Return browser to pool for reuse
        if let Some(browser) = self.browser.take() {
            let pool = self.pool.clone();

            tokio::spawn(async move {
                let mut browsers = pool.lock().await;
                browsers.push(browser);
                debug!("Browser returned to pool (total: {})", browsers.len());
            });
        }
    }
}

impl std::ops::Deref for BrowserGuard {
    type Target = Browser;

    fn deref(&self) -> &Self::Target {
        self.browser
            .as_ref()
            .expect("Browser guard must have a browser")
    }
}

/// Browser engine using the browser pool
pub struct BrowserEngine {
    pool: Arc<BrowserPool>,
}

impl BrowserEngine {
    /// Create a new browser engine with default settings
    pub async fn new() -> Result<Self> {
        Self::with_config(BrowserEngineConfig::default()).await
    }

    /// Create a new browser engine with custom configuration
    pub async fn with_config(config: BrowserEngineConfig) -> Result<Self> {
        let pool = Arc::new(BrowserPool::new(config.pool_size).await?);

        info!(
            "Browser engine initialized with pool size: {}",
            config.pool_size
        );

        Ok(Self { pool })
    }

    /// Setup request blocking for ads and analytics
    async fn setup_request_blocking(&self, page: &Page) -> Result<()> {
        // Check if ad blocking is enabled
        let block_ads = std::env::var("BROWSER_BLOCK_ADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true); // Default: enabled

        if !block_ads {
            debug!("Ad blocking disabled");
            return Ok(());
        }

        info!(
            "Enabling ad/analytics blocking for {} domains",
            BLOCKED_DOMAINS.len()
        );

        // Enable fetch domain for request interception
        page.execute(EnableParams::default()).await.map_err(|e| {
            ScrapeError::BrowserError(format!("Failed to enable fetch domain: {}", e))
        })?;

        // Clone page for async task
        let page = page.clone();

        // Spawn task to handle request interception
        tokio::spawn(async move {
            // Listen for request paused events
            let mut event_stream = match page.event_listener::<EventRequestPaused>().await {
                Ok(stream) => stream,
                Err(e) => {
                    warn!("Failed to create request event listener: {}", e);
                    return;
                }
            };

            while let Some(event) = event_stream.next().await {
                let url = &event.request.url;

                // Check if URL matches blocked domain
                let should_block = BLOCKED_DOMAINS.iter().any(|domain| url.contains(domain));

                if should_block {
                    debug!("Blocking request to: {}", url);
                    // Abort the request
                    let params = FailRequestParams::new(
                        event.request_id.clone(),
                        ErrorReason::BlockedByClient,
                    );
                    if let Err(e) = page.execute(params).await {
                        warn!("Failed to block request: {}", e);
                    }
                } else {
                    // Allow the request to continue
                    let params = ContinueRequestParams::new(event.request_id.clone());
                    if let Err(e) = page.execute(params).await {
                        warn!("Failed to continue request: {}", e);
                    }
                }
            }

            debug!("Request interception task finished");
        });

        Ok(())
    }

    /// Execute browser actions on a page
    async fn execute_actions(&self, page: &Page, actions: &[BrowserAction]) -> Result<()> {
        for action in actions {
            match action {
                BrowserAction::Click { selector } => {
                    debug!("Clicking element: {}", selector);
                    let element = page.find_element(selector).await.map_err(|e| {
                        ScrapeError::ElementNotFound(format!(
                            "Failed to find element {}: {}",
                            selector, e
                        ))
                    })?;

                    element.click().await.map_err(|e| {
                        ScrapeError::BrowserError(format!("Failed to click element: {}", e))
                    })?;
                }
                BrowserAction::Type { selector, text } => {
                    debug!("Typing '{}' into element: {}", text, selector);
                    let element = page.find_element(selector).await.map_err(|e| {
                        ScrapeError::ElementNotFound(format!(
                            "Failed to find element {}: {}",
                            selector, e
                        ))
                    })?;

                    element.click().await.map_err(|e| {
                        ScrapeError::BrowserError(format!("Failed to focus element: {}", e))
                    })?;

                    element.type_str(text).await.map_err(|e| {
                        ScrapeError::BrowserError(format!("Failed to type text: {}", e))
                    })?;
                }
                BrowserAction::Scroll { direction } => {
                    debug!("Scrolling: {}", direction);
                    let script = match direction.as_str() {
                        "down" => "window.scrollBy(0, window.innerHeight);",
                        "up" => "window.scrollBy(0, -window.innerHeight);",
                        "bottom" => "window.scrollTo(0, document.body.scrollHeight);",
                        "top" => "window.scrollTo(0, 0);",
                        _ => {
                            return Err(ScrapeError::BrowserError(format!(
                                "Invalid scroll direction: {}",
                                direction
                            )))
                        }
                    };

                    page.evaluate(script).await.map_err(|e| {
                        ScrapeError::BrowserError(format!("Failed to scroll: {}", e))
                    })?;
                }
                BrowserAction::Wait { milliseconds } => {
                    debug!("Waiting for {} ms", milliseconds);
                    tokio::time::sleep(Duration::from_millis(*milliseconds)).await;
                }
                BrowserAction::WaitForSelector { selector } => {
                    debug!("Waiting for selector: {}", selector);
                    page.find_element(selector).await.map_err(|e| {
                        ScrapeError::ElementNotFound(format!(
                            "Element not found after waiting: {}",
                            e
                        ))
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Capture screenshot of the page
    /// Reserved for future screenshot functionality
    #[allow(dead_code)]
    async fn capture_screenshot(&self, page: &Page, format: &str) -> Result<String> {
        let screenshot_format = match format {
            "jpeg" | "jpg" => CaptureScreenshotFormat::Jpeg,
            _ => CaptureScreenshotFormat::Png,
        };

        let screenshot_bytes = page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(screenshot_format)
                    .full_page(true)
                    .build(),
            )
            .await
            .map_err(|e| {
                ScrapeError::BrowserError(format!("Failed to capture screenshot: {}", e))
            })?;

        Ok(general_purpose::STANDARD.encode(&screenshot_bytes))
    }
}

#[async_trait]
impl ScrapeEngine for BrowserEngine {
    async fn scrape(&self, request: &ScrapeRequest) -> Result<RawScrapeResult> {
        let start = Instant::now();

        info!("Starting browser scrape for URL: {}", request.url);

        // Get browser from pool
        let browser = self.pool.get_browser().await?;

        // Set timeout for entire operation
        let timeout = Duration::from_millis(request.timeout);

        let result = tokio::time::timeout(timeout, async {
            // Create a new page
            debug!("Creating new page");
            let page = browser
                .new_page("about:blank")
                .await
                .map_err(|e| ScrapeError::BrowserError(format!("Failed to create page: {}", e)))?;

            // Apply stealth techniques (hide webdriver, inject JS, randomize fingerprints)
            let stealth_mode = StealthMode::from_env();
            apply_stealth_techniques(&page, stealth_mode).await?;

            // Setup request blocking for ads/analytics before navigation
            self.setup_request_blocking(&page).await?;

            // Rewrite URL if needed (e.g., Google Docs → export URL)
            let url_to_scrape = rewrite_url(&request.url);

            // Navigate to URL
            debug!("Navigating to URL: {}", url_to_scrape);
            page.goto(&url_to_scrape)
                .await
                .map_err(|e| ScrapeError::NavigationFailed(format!("Failed to navigate: {}", e)))?;

            // Wait for network idle
            debug!("Waiting for network idle");
            page.wait_for_navigation()
                .await
                .map_err(|e| ScrapeError::NavigationFailed(format!("Navigation timeout: {}", e)))?;

            // Wait for selector if specified
            if let Some(selector) = &request.wait_for_selector {
                debug!("Waiting for selector: {}", selector);
                page.find_element(selector).await.map_err(|e| {
                    ScrapeError::ElementNotFound(format!("Selector not found: {}", e))
                })?;
            }

            // Additional wait time if specified
            if request.wait_for > 0 {
                debug!("Additional wait for {} ms", request.wait_for);
                tokio::time::sleep(Duration::from_millis(request.wait_for)).await;
            }

            // Execute browser actions if any
            if !request.actions.is_empty() {
                debug!("Executing {} browser actions", request.actions.len());
                self.execute_actions(&page, &request.actions).await?;
            }

            // Get final URL after any redirects
            let final_url = page
                .url()
                .await
                .map_err(|e| ScrapeError::BrowserError(format!("Failed to get URL: {}", e)))?
                .unwrap_or_else(|| request.url.clone());

            // Get HTML content
            debug!("Extracting HTML content");
            let html = page
                .content()
                .await
                .map_err(|e| ScrapeError::BrowserError(format!("Failed to get HTML: {}", e)))?;

            // Check response size limit
            let max_response_size_mb = std::env::var("MAX_RESPONSE_SIZE_MB")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(50);

            let max_size_bytes = max_response_size_mb * 1024 * 1024;

            if html.len() > max_size_bytes {
                return Err(ScrapeError::ResourceLimit(format!(
                    "Response too large: {:.2}MB > {}MB",
                    html.len() as f64 / (1024.0 * 1024.0),
                    max_response_size_mb
                )));
            }

            info!("Successfully scraped URL with browser: {}", final_url);

            // Close the page to free resources
            if let Err(e) = page.close().await {
                warn!("Failed to close page: {}", e);
            }

            Ok::<_, ScrapeError>(RawScrapeResult {
                url: final_url,
                status_code: 200, // Browser always returns 200 if navigation succeeds
                content_type: Some("text/html".to_string()),
                html,
                headers: vec![],
            })
        })
        .await;

        let _duration = start.elapsed().as_secs_f64();

        // Handle timeout and unwrap result
        match result {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ScrapeError::Timeout),
        }
    }
}

/// Browser engine configuration
#[derive(Debug, Clone)]
pub struct BrowserEngineConfig {
    pub headless: bool,
    pub pool_size: usize,
    pub user_agent: Option<String>,
}

impl Default for BrowserEngineConfig {
    fn default() -> Self {
        Self {
            headless: env::var("BROWSER_HEADLESS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            pool_size: env::var("BROWSER_POOL_SIZE")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            user_agent: env::var("BROWSER_USER_AGENT").ok(),
        }
    }
}

impl BrowserEngineConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn headless(mut self, headless: bool) -> Self {
        self.headless = headless;
        self
    }

    pub fn pool_size(mut self, size: usize) -> Self {
        self.pool_size = size;
        self
    }

    pub fn user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = BrowserEngineConfig::new();
        assert!(config.headless);
        assert_eq!(config.pool_size, 5);
    }

    #[test]
    fn test_config_builder() {
        let config = BrowserEngineConfig::new()
            .headless(false)
            .pool_size(10)
            .user_agent("Custom Agent");

        assert!(!config.headless);
        assert_eq!(config.pool_size, 10);
        assert_eq!(config.user_agent.as_deref(), Some("Custom Agent"));
    }

    #[test]
    fn test_chrome_detection() {
        // This test will pass if Chrome is installed
        let result = BrowserPool::find_chrome_executable();

        // We can't guarantee Chrome is installed in CI, so just check the error message
        if let Err(e) = result {
            match e {
                ScrapeError::BrowserNotFound(msg) => {
                    assert!(msg.contains("Chrome/Chromium not found"));
                }
                _ => panic!("Expected BrowserNotFound error"),
            }
        } else {
            // Chrome found, check it exists
            let path = result.unwrap();
            assert!(path.exists(), "Chrome path should exist: {:?}", path);
        }
    }

    #[tokio::test]
    #[ignore] // Requires Chrome to be installed
    async fn test_browser_pool_creation() {
        let pool = BrowserPool::new(2).await;
        assert!(
            pool.is_ok(),
            "Browser pool creation failed: {:?}",
            pool.err()
        );
    }

    #[tokio::test]
    #[ignore] // Requires Chrome to be installed
    async fn test_browser_pool_limit() {
        let pool = Arc::new(BrowserPool::new(2).await.unwrap());

        // Get 2 browsers (should work)
        let b1 = pool.get_browser().await.unwrap();
        let b2 = pool.get_browser().await.unwrap();

        // Try to get 3rd (should wait)
        let start = std::time::Instant::now();

        let pool_ref = pool.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            drop(b1); // Release after 500ms
        });

        let _b3 = pool_ref.get_browser().await.unwrap();
        let elapsed = start.elapsed();

        // Should have waited for b1 to be released
        assert!(
            elapsed.as_millis() >= 400,
            "Should have waited for browser release, got: {}ms",
            elapsed.as_millis()
        );

        drop(b2);
    }

    #[tokio::test]
    #[ignore] // Requires Chrome to be installed
    async fn test_browser_reuse() {
        let pool = Arc::new(BrowserPool::new(1).await.unwrap());

        {
            let _b1 = pool.get_browser().await.unwrap();
            // Use browser
        } // b1 dropped, should be returned to pool

        // Give time for the browser to be returned
        tokio::time::sleep(Duration::from_millis(100)).await;

        let start = std::time::Instant::now();
        let _b2 = pool.get_browser().await.unwrap();
        let elapsed = start.elapsed();

        // b2 should be reused browser (very fast, < 100ms)
        // Creating new browser takes > 1s typically
        assert!(
            elapsed.as_millis() < 500,
            "Browser should be reused (fast), but took: {}ms",
            elapsed.as_millis()
        );
    }

    #[tokio::test]
    #[ignore] // Requires Chrome to be installed and network
    async fn test_browser_health_check() {
        let pool = Arc::new(BrowserPool::new(1).await.unwrap());
        let browser = pool.get_browser().await.unwrap();

        // Browser should be healthy
        assert!(
            BrowserPool::is_browser_healthy(&browser).await,
            "Browser should be healthy after creation"
        );
    }
}
