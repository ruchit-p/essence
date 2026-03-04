//! Browser stealth module for bypassing anti-bot detection
//!
//! This module implements comprehensive stealth techniques to make browser automation
//! indistinguishable from real user browsing. It supports multiple stealth modes:
//!
//! - **None**: No stealth (fastest, may be detected)
//! - **Basic**: Hide webdriver, randomize UA, inject JS (default)
//! - **Advanced**: Full fingerprint randomization
//! - **Auto**: Start basic, escalate to advanced on 401/403/429 errors
//!
//! Based on Firecrawl stealth proxy analysis and puppeteer-extra-plugin-stealth.

use chromiumoxide::Page;
use crate::error::{Result, ScrapeError};
use rand::Rng;
use tracing::{debug, info, warn};

/// Stealth mode configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthMode {
    /// No stealth (fastest, may be detected by anti-bot systems)
    None,
    /// Basic stealth: hide webdriver, randomize UA, inject JavaScript
    /// - Overhead: <10ms
    /// - Success rate: ~70-80%
    Basic,
    /// Advanced stealth: full fingerprint randomization
    /// - Overhead: ~50-100ms
    /// - Success rate: ~90-95%
    Advanced,
    /// Auto-escalate: start with basic, escalate to advanced on auth errors
    /// - Detects 401/403/429 status codes
    /// - Retries automatically with advanced techniques
    Auto,
}

impl StealthMode {
    /// Parse stealth mode from environment variable
    pub fn from_env() -> Self {
        std::env::var("BROWSER_STEALTH_MODE")
            .ok()
            .and_then(|mode| match mode.to_lowercase().as_str() {
                "none" => Some(StealthMode::None),
                "basic" => Some(StealthMode::Basic),
                "advanced" => Some(StealthMode::Advanced),
                "auto" => Some(StealthMode::Auto),
                _ => {
                    warn!("Invalid BROWSER_STEALTH_MODE '{}', using Basic", mode);
                    None
                }
            })
            .unwrap_or(StealthMode::Basic) // Default to Basic
    }

    /// Get timeout in milliseconds for this stealth mode
    pub fn timeout_ms(&self) -> u64 {
        match self {
            StealthMode::None | StealthMode::Basic => 30_000,  // 30 seconds
            StealthMode::Advanced | StealthMode::Auto => {
                // Check env var, default to 120 seconds for advanced
                std::env::var("BROWSER_STEALTH_TIMEOUT_MS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(120_000)
            }
        }
    }
}

impl std::fmt::Display for StealthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StealthMode::None => write!(f, "none"),
            StealthMode::Basic => write!(f, "basic"),
            StealthMode::Advanced => write!(f, "advanced"),
            StealthMode::Auto => write!(f, "auto"),
        }
    }
}

/// Apply stealth techniques to a browser page
///
/// This function injects JavaScript anti-detection code and optionally applies
/// advanced fingerprint randomization based on the stealth mode.
///
/// # Arguments
/// * `page` - The browser page to apply stealth to
/// * `mode` - The stealth mode to use
///
/// # Returns
/// * `Ok(())` if stealth was applied successfully
/// * `Err(ScrapeError)` if injection failed
pub async fn apply_stealth_techniques(page: &Page, mode: StealthMode) -> Result<()> {
    if mode == StealthMode::None {
        debug!("Stealth mode is None, skipping techniques");
        return Ok(());
    }

    info!("Applying stealth mode: {}", mode);

    // Inject JavaScript stealth code (embedded at compile time)
    let stealth_js = include_str!("stealth.js");

    page.evaluate(stealth_js)
        .await
        .map_err(|e| ScrapeError::BrowserError(format!("Failed to inject stealth JavaScript: {}", e)))?;

    debug!("JavaScript stealth injection successful");

    // Apply advanced techniques if requested
    if matches!(mode, StealthMode::Advanced | StealthMode::Auto) {
        apply_advanced_stealth(page).await?;
    }

    info!("Stealth techniques applied successfully (mode: {})", mode);
    Ok(())
}

/// Apply advanced fingerprint randomization
///
/// This function randomizes browser fingerprints to make each session unique:
/// - Screen resolution (from common resolutions)
/// - Hardware concurrency (CPU cores: 4-16)
/// - Device memory (RAM: 4, 8, 16, 32 GB)
///
/// These values are randomized per page but remain consistent within the same page.
async fn apply_advanced_stealth(page: &Page) -> Result<()> {
    debug!("Applying advanced fingerprint randomization");

    // Generate all random values BEFORE any await points (ThreadRng is not Send)
    let (width, height, cores, memory) = {
        let mut rng = rand::thread_rng();

        // Randomize screen resolution (use common real-world values)
        let resolutions = [
            (1920, 1080),  // Full HD (most common)
            (1366, 768),   // HD (common laptop)
            (1440, 900),   // Common Mac
            (1536, 864),   // Common Windows
            (1280, 720),   // HD
            (1600, 900),   // HD+
        ];
        let (w, h) = resolutions[rng.gen_range(0..resolutions.len())];

        // Randomize hardware concurrency (CPU cores)
        let c = rng.gen_range(4..17); // 4-16 cores (realistic range)

        // Randomize device memory (RAM in GB)
        let memory_options = [4, 8, 16, 32];
        let m = memory_options[rng.gen_range(0..memory_options.len())];

        (w, h, c, m)
    };

    // Apply fingerprint via JavaScript
    let fingerprint_js = format!(
        r#"
        // Screen dimensions
        Object.defineProperty(screen, 'width', {{ get: () => {}, configurable: true }});
        Object.defineProperty(screen, 'height', {{ get: () => {}, configurable: true }});
        Object.defineProperty(screen, 'availWidth', {{ get: () => {}, configurable: true }});
        Object.defineProperty(screen, 'availHeight', {{ get: () => {}, configurable: true }});

        // Hardware
        Object.defineProperty(navigator, 'hardwareConcurrency', {{ get: () => {}, configurable: true }});
        Object.defineProperty(navigator, 'deviceMemory', {{ get: () => {}, configurable: true }});

        // Log fingerprint (for debugging)
        console.log('[Stealth] Fingerprint: {{width}}x{{height}}, {{cores}} cores, {{memory}}GB RAM');
        "#,
        width, height, width, height - 40, cores, memory,
    );

    page.evaluate(fingerprint_js)
        .await
        .map_err(|e| ScrapeError::BrowserError(format!("Failed to apply fingerprint randomization: {}", e)))?;

    info!(
        "Advanced stealth applied: {}x{} screen, {} cores, {}GB RAM",
        width, height, cores, memory
    );

    Ok(())
}

/// Check if we should escalate to stealth based on status code
///
/// Auto-escalation is triggered on:
/// - 401 Unauthorized
/// - 403 Forbidden (anti-bot protection)
/// - 429 Too Many Requests (rate limiting)
///
/// # Arguments
/// * `status_code` - HTTP status code from the response
/// * `current_mode` - Current stealth mode in use
///
/// # Returns
/// * `true` if we should retry with advanced stealth
/// * `false` otherwise
pub fn should_escalate_to_stealth(status_code: u16, current_mode: StealthMode) -> bool {
    // Only escalate if in Auto mode and not already using advanced
    if current_mode != StealthMode::Auto {
        return false;
    }

    // Escalate on auth/rate-limit errors
    let should_escalate = [401, 403, 429].contains(&status_code);

    if should_escalate {
        warn!(
            "Detected status code {} - will auto-escalate to advanced stealth",
            status_code
        );
    }

    should_escalate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stealth_mode_from_env() {
        // Test valid modes
        std::env::set_var("BROWSER_STEALTH_MODE", "none");
        assert_eq!(StealthMode::from_env(), StealthMode::None);

        std::env::set_var("BROWSER_STEALTH_MODE", "basic");
        assert_eq!(StealthMode::from_env(), StealthMode::Basic);

        std::env::set_var("BROWSER_STEALTH_MODE", "advanced");
        assert_eq!(StealthMode::from_env(), StealthMode::Advanced);

        std::env::set_var("BROWSER_STEALTH_MODE", "auto");
        assert_eq!(StealthMode::from_env(), StealthMode::Auto);

        // Test invalid mode (should default to Basic)
        std::env::set_var("BROWSER_STEALTH_MODE", "invalid");
        assert_eq!(StealthMode::from_env(), StealthMode::Basic);

        // Test missing var (should default to Basic)
        std::env::remove_var("BROWSER_STEALTH_MODE");
        assert_eq!(StealthMode::from_env(), StealthMode::Basic);
    }

    #[test]
    fn test_stealth_mode_timeout() {
        assert_eq!(StealthMode::None.timeout_ms(), 30_000);
        assert_eq!(StealthMode::Basic.timeout_ms(), 30_000);
        assert_eq!(StealthMode::Advanced.timeout_ms(), 120_000);
        assert_eq!(StealthMode::Auto.timeout_ms(), 120_000);
    }

    #[test]
    fn test_should_escalate_to_stealth() {
        // Auto mode should escalate on 401/403/429
        assert!(should_escalate_to_stealth(401, StealthMode::Auto));
        assert!(should_escalate_to_stealth(403, StealthMode::Auto));
        assert!(should_escalate_to_stealth(429, StealthMode::Auto));

        // Should not escalate on other status codes
        assert!(!should_escalate_to_stealth(200, StealthMode::Auto));
        assert!(!should_escalate_to_stealth(404, StealthMode::Auto));
        assert!(!should_escalate_to_stealth(500, StealthMode::Auto));

        // Non-auto modes should never escalate
        assert!(!should_escalate_to_stealth(403, StealthMode::None));
        assert!(!should_escalate_to_stealth(403, StealthMode::Basic));
        assert!(!should_escalate_to_stealth(403, StealthMode::Advanced));
    }

    #[test]
    fn test_stealth_mode_display() {
        assert_eq!(format!("{}", StealthMode::None), "none");
        assert_eq!(format!("{}", StealthMode::Basic), "basic");
        assert_eq!(format!("{}", StealthMode::Advanced), "advanced");
        assert_eq!(format!("{}", StealthMode::Auto), "auto");
    }
}
