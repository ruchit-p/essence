use crate::{
    config::Settings,
    engines::{
        browser::BrowserEngine, detect_engine_needed, http::HttpEngine, racer::EngineRacer,
        EngineType, ScrapeEngine,
    },
    error::ScrapeError,
    format,
    types::{ScrapeRequest, ScrapeResponse},
    utils::robots,
    validation,
};
use axum::Json;
use tokio::sync::OnceCell;
use tracing::{error, info, warn};

/// Shared EngineRacer instance — initialized once, reused across all requests.
/// This ensures the BrowserPool is shared (browser reuse) and avoids the 2-5s
/// Chrome launch overhead on every request that triggers browser fallback.
static SHARED_RACER: OnceCell<EngineRacer> = OnceCell::const_new();

/// Get or initialize the shared EngineRacer
pub async fn get_shared_racer(delay_ms: u64) -> Result<&'static EngineRacer, ScrapeError> {
    SHARED_RACER
        .get_or_try_init(|| async {
            info!("Initializing shared EngineRacer (delay: {}ms)", delay_ms);
            EngineRacer::with_delay(delay_ms).await
        })
        .await
        .map_err(|e| {
            error!("Failed to initialize shared EngineRacer: {}", e);
            e
        })
}

/// Core scrape logic that can be called from both API handler and queue service
pub async fn scrape_core_logic(request: &ScrapeRequest) -> Result<ScrapeResponse, ScrapeError> {
    info!(
        "Scrape request received for URL: {} with engine: {}",
        request.url, request.engine
    );

    // Validate request (includes SSRF protection)
    validation::validate_scrape_request(request).await?;

    // Check robots.txt (optional, can be disabled)
    match robots::is_allowed_default(&request.url).await {
        Ok(allowed) => {
            if !allowed {
                warn!("Robots.txt disallows scraping for URL: {}", request.url);
                // For now, we'll allow it with a warning
                // In production, you might want to enforce this
            }
        }
        Err(e) => {
            warn!("Failed to check robots.txt: {}, continuing anyway", e);
        }
    }

    // Load settings to check if waterfall racing is enabled
    let settings = Settings::new().map_err(|e| {
        error!("Failed to load settings: {}", e);
        ScrapeError::Configuration(format!("Failed to load settings: {}", e))
    })?;

    // Determine which engine to use
    let use_browser = match request.engine.as_str() {
        "browser" => true,
        "http" => false,
        _ => {
            // Auto mode (default) - fast with content-quality-first fallback
            if settings.engine.waterfall_enabled {
                info!(
                    "Using waterfall racing for URL: {} (delay: {}ms)",
                    request.url, settings.engine.waterfall_delay_ms
                );

                let racer = get_shared_racer(settings.engine.waterfall_delay_ms).await?;

                let (raw_result, metrics) =
                    racer.race_scrape_with_metrics(request).await.map_err(|e| {
                        error!("Waterfall race failed for URL {}: {}", request.url, e);
                        e
                    })?;

                info!(
                    "Waterfall race completed: winner={}, elapsed={}ms, browser_started={}",
                    metrics.winning_engine, metrics.elapsed_ms, metrics.browser_started
                );

                // Process the result
                let document = format::process_scrape_result(raw_result, request)
                    .await
                    .map_err(|e| {
                        error!("Failed to process scrape result: {}", e);
                        e
                    })?;

                info!("Successfully processed document for URL: {}", request.url);
                return Ok(ScrapeResponse::success(document));
            } else {
                // Waterfall disabled - use legacy sequential fallback
                info!("Auto-detecting engine type (waterfall disabled)...");

                let http_engine =
                    HttpEngine::with_options(request.timeout, request.skip_tls_verification)
                        .map_err(|e| {
                            error!("Failed to create HTTP engine: {}", e);
                            e
                        })?;

                let http_result = http_engine.scrape(request).await.map_err(|e| {
                    error!(
                        "Failed to scrape URL with HTTP engine {}: {}",
                        request.url, e
                    );
                    e
                })?;

                let detected_engine = detect_engine_needed(&http_result.url, &http_result.html);

                if detected_engine == EngineType::Browser {
                    info!(
                        "Auto-detection recommends Browser engine for URL: {}",
                        request.url
                    );
                    true
                } else {
                    info!(
                        "Auto-detection recommends HTTP engine for URL: {}",
                        request.url
                    );

                    // Process HTTP result and return early
                    let document = format::process_scrape_result(http_result, request)
                        .await
                        .map_err(|e| {
                            error!("Failed to process scrape result: {}", e);
                            e
                        })?;

                    info!("Successfully processed document for URL: {}", request.url);
                    return Ok(ScrapeResponse::success(document));
                }
            }
        }
    };

    // Use browser engine if needed or requested
    if use_browser {
        info!("Using Browser engine for URL: {}", request.url);

        let browser_engine = BrowserEngine::new().await.map_err(|e| {
            error!("Failed to create browser engine: {}", e);
            e
        })?;

        let raw_result = browser_engine.scrape(request).await.map_err(|e| {
            error!("Failed to scrape URL with browser {}: {}", request.url, e);
            e
        })?;

        info!(
            "Successfully fetched URL with browser: {} (status: {})",
            raw_result.url, raw_result.status_code
        );

        // Capture screenshot if requested
        let screenshot = if request.screenshot {
            info!("Capturing screenshot...");
            // Screenshot logic would go here
            // For now, we'll skip it in the main flow
            None
        } else {
            None
        };

        // Process the result into the requested formats
        let mut document = format::process_scrape_result(raw_result, request)
            .await
            .map_err(|e| {
                error!("Failed to process scrape result: {}", e);
                e
            })?;

        // Add screenshot to document if captured
        if let Some(screenshot_data) = screenshot {
            document.screenshot = Some(screenshot_data);
        }

        info!("Successfully processed document for URL: {}", request.url);
        Ok(ScrapeResponse::success(document))
    } else {
        // This branch shouldn't be reached due to early return in auto mode
        info!("Using HTTP engine for URL: {}", request.url);

        let http_engine = HttpEngine::with_options(request.timeout, request.skip_tls_verification)
            .map_err(|e| {
                error!("Failed to create HTTP engine: {}", e);
                e
            })?;

        let raw_result = http_engine.scrape(request).await.map_err(|e| {
            error!("Failed to scrape URL {}: {}", request.url, e);
            e
        })?;

        info!(
            "Successfully fetched URL: {} (status: {})",
            raw_result.url, raw_result.status_code
        );

        if raw_result.status_code >= 400 {
            warn!("URL returned error status code: {}", raw_result.status_code);
        }

        let document = format::process_scrape_result(raw_result, request)
            .await
            .map_err(|e| {
                error!("Failed to process scrape result: {}", e);
                e
            })?;

        info!("Successfully processed document for URL: {}", request.url);
        Ok(ScrapeResponse::success(document))
    }
}

/// Handler for POST /api/v1/scrape
pub async fn scrape_handler(
    Json(request): Json<ScrapeRequest>,
) -> Result<Json<ScrapeResponse>, ScrapeError> {
    let response = scrape_core_logic(&request).await?;
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scrape_handler_invalid_url() {
        let request = ScrapeRequest {
            url: "".to_string(),
            formats: vec!["markdown".to_string()],
            headers: Default::default(),
            include_tags: vec![],
            exclude_tags: vec![],
            only_main_content: true,
            timeout: 30000,
            wait_for: 0,
            remove_base64_images: true,
            skip_tls_verification: false,
            engine: "auto".to_string(),
            wait_for_selector: None,
            actions: vec![],
            screenshot: false,
            screenshot_format: "png".to_string(),
        };

        let result = scrape_handler(Json(request)).await;
        assert!(result.is_err());
    }
}
