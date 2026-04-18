use crate::{
    engines::{browser::BrowserEngine, http::HttpEngine, RawScrapeResult, ScrapeEngine},
    error::Result,
    types::ScrapeRequest,
};
use std::time::{Duration, Instant};
use tokio::select;
use tracing::{debug, info, warn};

/// Metrics for racer results
#[derive(Debug, Clone)]
pub struct RacerMetrics {
    /// Which engine won the race
    pub winning_engine: String,
    /// Total time elapsed
    pub elapsed_ms: u64,
    /// Whether browser was started
    pub browser_started: bool,
    /// HTTP engine status
    pub http_status: EngineStatus,
    /// Browser engine status (if started)
    pub browser_status: Option<EngineStatus>,
}

#[derive(Debug, Clone)]
pub enum EngineStatus {
    Success { duration_ms: u64 },
    Failed { duration_ms: u64, error: String },
    NotStarted,
    Cancelled,
}

/// Engine waterfall racer - races engines with staggered starts
///
/// This implements a Firecrawl-style waterfall racing strategy:
/// 1. Start HTTP engine immediately
/// 2. If HTTP doesn't complete in `waterfall_delay_ms`, start browser
/// 3. Return first successful result (with quality validation)
/// 4. Cancel slower engines automatically via tokio::select!
/// 5. Track metrics for debugging and optimization
pub struct EngineRacer {
    http_engine: HttpEngine,
    browser_engine: BrowserEngine,
    waterfall_delay: Duration,
    validate_quality: bool,
    /// Visible text threshold above which HTTP results are accepted.
    /// Maps to CONTENT_SUFFICIENT_CHARS env var / config.rs.
    content_sufficient_chars: usize,
}

impl EngineRacer {
    /// Create a new engine racer with default settings
    pub async fn new() -> Result<Self> {
        let waterfall_delay = Duration::from_millis(
            std::env::var("ENGINE_WATERFALL_DELAY_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1500), // 1.5s default
        );
        let content_sufficient_chars: usize = std::env::var("CONTENT_SUFFICIENT_CHARS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);

        Ok(Self {
            http_engine: HttpEngine::new()?,
            browser_engine: BrowserEngine::new().await?,
            waterfall_delay,
            validate_quality: true,
            content_sufficient_chars,
        })
    }

    /// Create a new engine racer with custom delay
    pub async fn with_delay(delay_ms: u64) -> Result<Self> {
        Self::with_config(delay_ms, true, 1000).await
    }

    /// Create racer with custom options
    pub async fn with_options(delay_ms: u64, validate_quality: bool) -> Result<Self> {
        Self::with_config(delay_ms, validate_quality, 1000).await
    }

    /// Create racer with full config
    pub async fn with_config(
        delay_ms: u64,
        validate_quality: bool,
        content_sufficient_chars: usize,
    ) -> Result<Self> {
        Ok(Self {
            http_engine: HttpEngine::new()?,
            browser_engine: BrowserEngine::new().await?,
            waterfall_delay: Duration::from_millis(delay_ms),
            validate_quality,
            content_sufficient_chars,
        })
    }

    /// Race engines with waterfall timeout
    ///
    /// Strategy:
    /// 1. Start HTTP engine immediately
    /// 2. If HTTP doesn't complete in `waterfall_delay`, start browser in parallel
    /// 3. Return first successful result (with quality validation if enabled)
    /// 4. If HTTP fails early, still race both engines for best result
    /// 5. Slower engines are automatically cancelled by tokio::select!
    /// 6. Track detailed metrics for debugging
    pub async fn race_scrape(&self, request: &ScrapeRequest) -> Result<RawScrapeResult> {
        let start_time = Instant::now();

        debug!(
            "Starting waterfall race with {}ms delay for URL: {}",
            self.waterfall_delay.as_millis(),
            request.url
        );

        // Start HTTP engine immediately
        let http_start = Instant::now();
        let http_future = self.http_engine.scrape(request);
        tokio::pin!(http_future);

        // Wait for either HTTP to complete or waterfall timeout
        let http_result = select! {
            result = &mut http_future => {
                let http_duration = http_start.elapsed();
                debug!("HTTP engine completed in {}ms", http_duration.as_millis());
                Some((result, http_duration))
            }
            _ = tokio::time::sleep(self.waterfall_delay) => {
                debug!("HTTP engine timeout ({}ms), starting browser engine", self.waterfall_delay.as_millis());
                None
            }
        };

        // Check if we should return the HTTP result early
        let http_completed = http_result.is_some();

        // If HTTP completed before timeout, validate and potentially return it
        if let Some((result, http_duration)) = http_result {
            match result {
                Ok(raw) => {
                    // Check for blocking/error status codes first
                    if should_fallback_to_browser(&raw) {
                        warn!(
                            "HTTP returned blocking/error status {} in {}ms, racing with browser",
                            raw.status_code,
                            http_duration.as_millis()
                        );
                        // Fall through to race with browser
                    } else if self.validate_quality {
                        // Content-quality-first: distinguish main content from nav chrome.
                        let quality = extract_content_quality(&raw.html);
                        let threshold = self.content_sufficient_chars;
                        let low_threshold = threshold / 5;

                        if quality.main_content_chars > threshold {
                            info!(
                                "HTTP engine won the race ({}ms) with sufficient main content ({} chars, nav_ratio: {:.2})",
                                http_duration.as_millis(), quality.main_content_chars, quality.nav_ratio
                            );
                            return Ok(raw);
                        }
                        if quality.total_visible_chars > threshold && quality.nav_ratio < 0.7 {
                            info!(
                                "HTTP engine won the race ({}ms) with sufficient total content ({} chars, nav_ratio: {:.2})",
                                http_duration.as_millis(), quality.total_visible_chars, quality.nav_ratio
                            );
                            return Ok(raw);
                        }
                        // Low main content — check if it's a true SPA shell
                        let detection =
                            crate::engines::detection::RenderingDetector::needs_javascript(
                                &raw.html,
                                &request.url,
                            );
                        if detection.needs_js {
                            info!(
                                "Low main content ({} chars, nav_ratio: {:.2}) + SPA/JS detected ({}), racing with browser for {}",
                                quality.main_content_chars, quality.nav_ratio, detection.reason, request.url
                            );
                            // Fall through to race with browser
                        } else if quality.main_content_chars > low_threshold {
                            info!(
                                "HTTP engine won the race ({}ms) with adequate quality ({} main chars)",
                                http_duration.as_millis(),
                                quality.main_content_chars
                            );
                            return Ok(raw);
                        } else {
                            warn!(
                                "HTTP result has low quality (main content: {} chars, total: {} chars), racing with browser",
                                quality.main_content_chars, quality.total_visible_chars
                            );
                            // Fall through to race with browser
                        }
                    } else {
                        info!("HTTP engine won the race ({}ms)", http_duration.as_millis());
                        return Ok(raw);
                    }
                }
                Err(e) => {
                    warn!(
                        "HTTP engine failed in {}ms: {}, racing with browser",
                        http_duration.as_millis(),
                        e
                    );
                    // Fall through to race with browser
                }
            }
        }

        // At this point, either:
        // 1. HTTP timed out (still running)
        // 2. HTTP failed or had low quality
        // Race both engines and take the first successful result

        // When falling back to browser, set wait based on reason:
        // - True SPA (low content): 2000ms for hydration
        // - Status code fallback (403/429): 500ms, just need page load
        // - Other: 1000ms moderate wait
        let mut browser_request = request.clone();
        if browser_request.wait_for == 0 {
            browser_request.wait_for = 1000;
        }

        let browser_start = Instant::now();
        let browser_future = self.browser_engine.scrape(&browser_request);

        let (winning_result, winning_engine) = if !http_completed {
            // HTTP is still running, race it with browser
            select! {
                result = http_future => {
                    let duration = http_start.elapsed();
                    info!("HTTP engine completed after waterfall ({}ms)", duration.as_millis());
                    (result, "http_late")
                }
                result = browser_future => {
                    let duration = browser_start.elapsed();
                    info!("Browser engine won the race ({}ms)", duration.as_millis());
                    (result, "browser")
                }
            }
        } else {
            // HTTP already completed but failed/low quality, just use browser
            let result = browser_future.await;
            let duration = browser_start.elapsed();
            info!(
                "Browser engine used as fallback ({}ms)",
                duration.as_millis()
            );
            (result, "browser_fallback")
        };

        let total_elapsed = start_time.elapsed();
        debug!(
            "Race completed in {}ms, winner: {}",
            total_elapsed.as_millis(),
            winning_engine
        );

        winning_result
    }

    /// Race engines and return result with metrics
    pub async fn race_scrape_with_metrics(
        &self,
        request: &ScrapeRequest,
    ) -> Result<(RawScrapeResult, RacerMetrics)> {
        let start_time = Instant::now();
        let mut http_status = EngineStatus::NotStarted;

        debug!(
            "Starting waterfall race with metrics for URL: {}",
            request.url
        );

        // Start HTTP engine
        let http_start = Instant::now();
        let http_future = self.http_engine.scrape(request);
        tokio::pin!(http_future);

        // Wait for HTTP or timeout
        let http_result = select! {
            result = &mut http_future => {
                let duration = http_start.elapsed();
                http_status = match &result {
                    Ok(_) => EngineStatus::Success { duration_ms: duration.as_millis() as u64 },
                    Err(e) => EngineStatus::Failed {
                        duration_ms: duration.as_millis() as u64,
                        error: e.to_string()
                    },
                };
                Some(result)
            }
            _ = tokio::time::sleep(self.waterfall_delay) => None
        };

        // Early HTTP success check - validate status code AND content quality
        let should_continue_to_browser = if let Some(Ok(ref raw)) = http_result {
            // Check if we should fallback to browser for error/blocking status codes
            if should_fallback_to_browser(raw) {
                warn!(
                    "HTTP returned blocking/error status {}, falling back to browser engine",
                    raw.status_code
                );
                true // Continue to browser fallback
            } else if self.validate_quality {
                // Content-quality-first: distinguish main content from nav chrome.
                let quality = extract_content_quality(&raw.html);
                let threshold = self.content_sufficient_chars;
                let low_threshold = threshold / 5;

                if quality.main_content_chars > threshold {
                    info!(
                        "HTTP succeeded with sufficient main content ({} chars, nav_ratio: {:.2}), skipping browser for {}",
                        quality.main_content_chars, quality.nav_ratio, request.url
                    );
                    false
                } else if quality.total_visible_chars > threshold && quality.nav_ratio < 0.7 {
                    info!(
                        "HTTP succeeded with sufficient total content ({} chars, nav_ratio: {:.2}), skipping browser for {}",
                        quality.total_visible_chars, quality.nav_ratio, request.url
                    );
                    false
                } else {
                    // Low main content — check if it's a true SPA shell
                    let detection = crate::engines::detection::RenderingDetector::needs_javascript(
                        &raw.html,
                        &request.url,
                    );
                    if detection.needs_js {
                        info!(
                            "Low main content ({} chars, nav_ratio: {:.2}) + SPA/JS detected ({}), falling back to browser for {}",
                            quality.main_content_chars, quality.nav_ratio, detection.reason, request.url
                        );
                        true
                    } else if quality.main_content_chars > low_threshold {
                        false
                    } else {
                        warn!(
                            "HTTP result has low quality (main content: {} chars, total: {} chars), falling back to browser",
                            quality.main_content_chars, quality.total_visible_chars
                        );
                        true
                    }
                }
            } else {
                false
            }
        } else {
            // HTTP failed or timed out, need browser
            true
        };

        if !should_continue_to_browser {
            // HTTP succeeded with good status, return it
            if let Some(Ok(raw)) = http_result {
                let metrics = RacerMetrics {
                    winning_engine: "http".to_string(),
                    elapsed_ms: start_time.elapsed().as_millis() as u64,
                    browser_started: false,
                    http_status,
                    browser_status: None,
                };
                return Ok((raw, metrics));
            }
        }

        // Start browser — set moderate wait (not 2s for everything)
        let mut browser_request = request.clone();
        if browser_request.wait_for == 0 {
            browser_request.wait_for = 1000;
        }
        let browser_start = Instant::now();
        let browser_future = self.browser_engine.scrape(&browser_request);

        // Race remaining futures
        // If HTTP timed out (http_result is None), race both futures
        // If HTTP completed but we're falling back, just use browser
        let (result, winning_engine, browser_status) = if http_result.is_none() {
            // HTTP is still running, race it with browser
            select! {
                result = http_future => {
                    let duration = http_start.elapsed();
                    http_status = match &result {
                        Ok(_) => EngineStatus::Success { duration_ms: duration.as_millis() as u64 },
                        Err(e) => EngineStatus::Failed {
                            duration_ms: duration.as_millis() as u64,
                            error: e.to_string()
                        },
                    };
                    (result, "http_late", Some(EngineStatus::Cancelled))
                }
                result = browser_future => {
                    let duration = browser_start.elapsed();
                    let status = match &result {
                        Ok(_) => EngineStatus::Success { duration_ms: duration.as_millis() as u64 },
                        Err(e) => EngineStatus::Failed {
                            duration_ms: duration.as_millis() as u64,
                            error: e.to_string()
                        },
                    };
                    (result, "browser", Some(status))
                }
            }
        } else {
            let result = browser_future.await;
            let duration = browser_start.elapsed();
            let status = match &result {
                Ok(_) => EngineStatus::Success {
                    duration_ms: duration.as_millis() as u64,
                },
                Err(e) => EngineStatus::Failed {
                    duration_ms: duration.as_millis() as u64,
                    error: e.to_string(),
                },
            };
            (result, "browser_fallback", Some(status))
        };

        let metrics = RacerMetrics {
            winning_engine: winning_engine.to_string(),
            elapsed_ms: start_time.elapsed().as_millis() as u64,
            browser_started: true,
            http_status,
            browser_status,
        };

        result.map(|r| (r, metrics))
    }
}

/// Check if we should fallback to browser engine based on HTTP response
fn should_fallback_to_browser(raw: &RawScrapeResult) -> bool {
    // Status codes that indicate blocking, authentication, or anti-bot protection
    match raw.status_code {
        401 | 403 => {
            // Unauthorized or Forbidden - likely anti-bot or auth required
            info!(
                "Detected blocking status code {}, will try browser fallback",
                raw.status_code
            );
            true
        }
        429 => {
            // Rate limited - browser might help with different fingerprint
            info!("Detected rate limit (429), will try browser fallback");
            true
        }
        503 => {
            // Service unavailable - might be anti-bot protection
            info!("Detected service unavailable (503), will try browser fallback");
            true
        }
        _ if raw.status_code >= 400 => {
            // Other client/server errors - check if page looks like anti-bot
            let html_lower = raw.html.to_lowercase();
            let is_blocking_page = html_lower.contains("access denied")
                || html_lower.contains("blocked")
                || html_lower.contains("captcha")
                || html_lower.contains("cloudflare")
                || html_lower.contains("challenge")
                || html_lower.contains("please verify")
                || html_lower.contains("bot detection");

            if is_blocking_page {
                info!("Detected anti-bot page content, will try browser fallback");
            }
            is_blocking_page
        }
        _ => false,
    }
}

/// Content quality analysis distinguishing navigation chrome from main content.
struct ContentQuality {
    /// Total visible text chars (excluding script/style/noscript)
    total_visible_chars: usize,
    /// Text chars NOT inside nav/header/footer/aside elements
    main_content_chars: usize,
    /// Fraction of visible text that is navigation/chrome (0.0 - 1.0)
    nav_ratio: f64,
}

/// Analyze content quality from HTML, distinguishing nav chrome from main content.
/// This prevents SPA shells with massive navigation but JS-rendered body from
/// passing the quality check.
fn extract_content_quality(html: &str) -> ContentQuality {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);

    // Select the body, fall back to root
    let body_selector = Selector::parse("body").ok();
    let root = body_selector
        .as_ref()
        .and_then(|s| document.select(s).next())
        .unwrap_or_else(|| document.root_element());

    let skip_selector = Selector::parse("script, style, noscript").ok();
    let nav_selector = Selector::parse(
        "nav, header, footer, aside, [role='navigation'], [role='banner'], [role='contentinfo']",
    )
    .ok();

    // Collect IDs of elements to skip (script/style/noscript)
    let skip_ids: std::collections::HashSet<_> = skip_selector
        .as_ref()
        .map(|s| document.select(s).map(|el| el.id()).collect())
        .unwrap_or_default();

    // Collect IDs of nav/chrome elements
    let nav_ids: std::collections::HashSet<_> = nav_selector
        .as_ref()
        .map(|s| document.select(s).map(|el| el.id()).collect())
        .unwrap_or_default();

    let mut total_text = String::new();
    let mut nav_text = String::new();

    for node in root.descendants() {
        if let Some(el) = node.value().as_element() {
            if skip_ids.contains(&node.id()) || matches!(el.name(), "script" | "style" | "noscript")
            {
                continue;
            }
        }
        if let Some(t) = node.value().as_text() {
            // Check if this text node is inside a skip element
            let mut parent = node.parent();
            let mut in_skip = false;
            let mut in_nav = false;
            while let Some(p) = parent {
                if let Some(el) = p.value().as_element() {
                    if matches!(el.name(), "script" | "style" | "noscript") {
                        in_skip = true;
                        break;
                    }
                }
                if nav_ids.contains(&p.id()) {
                    in_nav = true;
                }
                parent = p.parent();
            }
            if !in_skip {
                total_text.push_str(t);
                if in_nav {
                    nav_text.push_str(t);
                }
            }
        }
    }

    let total_visible_chars = total_text.trim().len();
    let nav_chars = nav_text.trim().len();
    let main_content_chars = total_visible_chars.saturating_sub(nav_chars);
    let nav_ratio = if total_visible_chars > 0 {
        nav_chars as f64 / total_visible_chars as f64
    } else {
        0.0
    };

    ContentQuality {
        total_visible_chars,
        main_content_chars,
        nav_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ScrapeRequest;

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_http_wins_race() {
        let racer = EngineRacer::new().await.unwrap();
        let request = ScrapeRequest {
            url: "https://example.com".to_string(),
            engine: "auto".to_string(),
            formats: vec!["markdown".to_string()],
            ..Default::default()
        };

        let result = racer.race_scrape(&request).await;
        assert!(result.is_ok(), "HTTP-friendly site should succeed");

        let raw = result.unwrap();
        assert!(raw.html.len() > 0, "Should return HTML content");
        assert_eq!(raw.status_code, 200, "Should return 200 status");
    }

    #[tokio::test]
    #[ignore] // Requires network and browser
    async fn test_browser_wins_race() {
        // Use a SPA-heavy site that needs browser rendering
        let racer = EngineRacer::new().await.unwrap();
        let request = ScrapeRequest {
            url: "https://react.dev".to_string(), // React docs are a SPA
            engine: "auto".to_string(),
            formats: vec!["markdown".to_string()],
            ..Default::default()
        };

        let result = racer.race_scrape(&request).await;
        assert!(result.is_ok(), "SPA site should succeed with browser");

        let raw = result.unwrap();
        assert!(raw.html.len() > 0, "Should return HTML content");
    }

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_waterfall_timing() {
        // Set a very short waterfall delay to test the mechanism
        let racer = EngineRacer::with_delay(100).await.unwrap(); // 100ms delay

        let request = ScrapeRequest {
            url: "https://example.com".to_string(),
            engine: "auto".to_string(),
            formats: vec!["markdown".to_string()],
            ..Default::default()
        };

        let start = std::time::Instant::now();
        let result = racer.race_scrape(&request).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "Request should succeed");

        // HTTP should win quickly (< 5s for example.com)
        assert!(
            elapsed.as_secs() < 5,
            "HTTP should complete quickly, took: {}ms",
            elapsed.as_millis()
        );
    }

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_http_failure_fallback() {
        let racer = EngineRacer::new().await.unwrap();

        // Use an invalid URL that will fail quickly on HTTP
        let request = ScrapeRequest {
            url: "https://this-domain-does-not-exist-essence-test-12345.com".to_string(),
            engine: "auto".to_string(),
            formats: vec!["markdown".to_string()],
            ..Default::default()
        };

        let result = racer.race_scrape(&request).await;
        // Both engines should fail for a non-existent domain
        assert!(result.is_err(), "Should fail for non-existent domain");
    }

    #[tokio::test]
    async fn test_racer_creation() {
        let racer = EngineRacer::new().await;
        assert!(racer.is_ok(), "Racer creation should succeed");
    }

    #[tokio::test]
    async fn test_racer_with_custom_delay() {
        let racer = EngineRacer::with_delay(3000).await;
        assert!(
            racer.is_ok(),
            "Racer creation with custom delay should succeed"
        );

        let racer = racer.unwrap();
        assert_eq!(
            racer.waterfall_delay.as_millis(),
            3000,
            "Should use custom delay"
        );
    }

    #[tokio::test]
    async fn test_racer_with_config() {
        let racer = EngineRacer::with_config(2000, true, 500).await;
        assert!(racer.is_ok(), "Racer creation with config should succeed");

        let racer = racer.unwrap();
        assert_eq!(racer.waterfall_delay.as_millis(), 2000);
        assert_eq!(racer.content_sufficient_chars, 500);
    }

    #[test]
    fn test_content_quality_with_nav_heavy_page() {
        let html = r#"<html><body>
            <nav>Home About Contact Blog Products Services Team Careers Press FAQ Support</nav>
            <header>Welcome to Our Company - Innovation at Scale</header>
            <main><p>Short body.</p></main>
            <footer>Copyright 2024 All Rights Reserved Terms Privacy</footer>
        </body></html>"#;

        let quality = extract_content_quality(html);
        assert!(
            quality.nav_ratio > 0.5,
            "Nav-heavy page should have high nav_ratio: {}",
            quality.nav_ratio
        );
        assert!(
            quality.main_content_chars < quality.total_visible_chars,
            "Main content should be less than total"
        );
    }

    #[test]
    fn test_content_quality_with_content_rich_page() {
        let html = r#"<html><body>
            <nav>Home About</nav>
            <main>
                <h1>Understanding Rust Ownership</h1>
                <p>Ownership is Rust's most unique feature and has deep implications for the rest of the language.
                It enables Rust to make memory safety guarantees without needing a garbage collector.
                In this chapter, we'll talk about ownership as well as several related features:
                borrowing, slices, and how Rust lays data out in memory. This is a substantial article
                with plenty of content that should demonstrate high content quality scores.</p>
            </main>
        </body></html>"#;

        let quality = extract_content_quality(html);
        assert!(
            quality.nav_ratio < 0.3,
            "Content-rich page should have low nav_ratio: {}",
            quality.nav_ratio
        );
        assert!(
            quality.main_content_chars > 200,
            "Should have substantial main content: {}",
            quality.main_content_chars
        );
    }

    #[test]
    fn test_content_quality_excludes_scripts() {
        let html = r#"<html><body>
            <script>var x = "lots of javascript code that should not count"; var y = "more code";</script>
            <style>.lots { of: css; rules: here; }</style>
            <p>Small visible text.</p>
        </body></html>"#;

        let quality = extract_content_quality(html);
        assert!(
            quality.total_visible_chars < 100,
            "Should exclude script/style text: {}",
            quality.total_visible_chars
        );
    }
}
