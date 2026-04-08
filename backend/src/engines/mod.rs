pub mod browser;
pub mod detection;
pub mod http;
pub mod racer;
pub mod stealth;

use crate::{error::Result, types::ScrapeRequest};
use async_trait::async_trait;
use detection::{DetectionResult, RenderingDetector};
use tracing::debug;

/// Raw scrape result before formatting
#[derive(Debug, Clone)]
pub struct RawScrapeResult {
    /// Final URL after redirects
    pub url: String,
    /// HTTP status code
    pub status_code: u16,
    /// Content-Type header
    pub content_type: Option<String>,
    /// Raw HTML content
    pub html: String,
    /// Response headers
    pub headers: Vec<(String, String)>,
}

/// Trait for scraping engines
#[async_trait]
pub trait ScrapeEngine: Send + Sync {
    /// Scrape a URL and return raw results
    async fn scrape(&self, request: &ScrapeRequest) -> Result<RawScrapeResult>;
}

/// Engine types
#[derive(Debug, Clone, PartialEq)]
pub enum EngineType {
    /// Standard HTTP engine
    Http,
    /// Browser-based engine for JavaScript-heavy sites
    Browser,
}

/// Detect which engine to use based on URL and HTML content
pub fn detect_engine_needed(url: &str, html: &str) -> EngineType {
    debug!("Detecting engine for URL: {}", url);

    // Use the new RenderingDetector for more sophisticated analysis
    let detection_result = RenderingDetector::needs_javascript(html, url);

    if detection_result.needs_js {
        debug!(
            "JavaScript rendering needed: {} (frameworks: {:?})",
            detection_result.reason, detection_result.detected_frameworks
        );
        return EngineType::Browser;
    }

    debug!(
        "No JavaScript rendering needed: {}",
        detection_result.reason
    );
    EngineType::Http
}

/// Get detailed detection result (for metadata/debugging)
pub fn detect_engine_with_reason(url: &str, html: &str) -> (EngineType, DetectionResult) {
    let detection_result = RenderingDetector::needs_javascript(html, url);
    let engine_type = if detection_result.needs_js {
        EngineType::Browser
    } else {
        EngineType::Http
    };
    (engine_type, detection_result)
}

// Old detection functions removed - now using RenderingDetector from detection module

/// Validate that scrape result contains meaningful content
pub fn validate_scrape_quality(result: &RawScrapeResult, markdown: &str) -> Result<()> {
    use crate::error::ScrapeError;

    // Check status code
    let is_good_status = (200..300).contains(&result.status_code) || result.status_code == 304; // Not Modified is OK

    // Check markdown content length
    let has_content = markdown.trim().len() > 100;

    // Calculate content density (text vs HTML ratio)
    let content_density = calculate_content_density(&result.html);

    // Check for error indicators
    let looks_like_error = is_likely_error_page(&result.html, result.status_code);

    if !has_content {
        return Err(ScrapeError::EmptyContent(format!(
            "Markdown output is too short (length: {})",
            markdown.len()
        )));
    }

    if content_density < 0.05 && !is_good_status {
        return Err(ScrapeError::LowQuality(format!(
            "Very low content density: {:.2}% with status {}",
            content_density * 100.0,
            result.status_code
        )));
    }

    if looks_like_error {
        return Err(ScrapeError::ErrorPage(format!(
            "Page appears to be an error page (status: {})",
            result.status_code
        )));
    }

    Ok(())
}

/// Calculate text content density (text length / HTML length)
fn calculate_content_density(html: &str) -> f64 {
    use scraper::Html;

    let document = Html::parse_document(html);

    // Extract all text
    let text = document.root_element().text().collect::<String>();

    let text_len = text.trim().len() as f64;
    let html_len = html.len() as f64;

    if html_len > 0.0 {
        text_len / html_len
    } else {
        0.0
    }
}

/// Detect if page is likely an error page
fn is_likely_error_page(html: &str, status_code: u16) -> bool {
    // For error status codes, it's definitely an error page
    if status_code >= 400 {
        return true;
    }

    // For successful status codes (200-299), be VERY conservative
    // Only flag as error if we have strong evidence
    if (200..300).contains(&status_code) {
        // Check for valid page metadata - if present, likely NOT an error page
        if has_valid_page_metadata(html) {
            return false;
        }

        // Check for error indicators in prominent places (title, headings)
        // Not in script tags or JSON data
        let title_indicators = [
            "<title>404",
            "<title>error",
            "<title>not found",
            "<title>access denied",
            "<title>forbidden",
        ];

        let lower = html.to_lowercase();

        // Count how many strong error indicators we find
        let mut error_count = 0;

        // Check title for error indicators
        if title_indicators
            .iter()
            .any(|&indicator| lower.contains(indicator))
        {
            error_count += 1;
        }

        // Check for prominent error messages in heading tags
        let heading_indicators = [
            "<h1>404",
            "<h1>error",
            "<h1>not found",
            "<h1>access denied",
            "<h2>404",
            "<h2>error",
            "<h2>not found",
        ];

        if heading_indicators
            .iter()
            .any(|&indicator| lower.contains(indicator))
        {
            error_count += 1;
        }

        // Check for very specific error phrases that are unlikely to appear in JS code
        let body_indicators = [
            "this page doesn't exist",
            "the page you are looking for does not exist",
            "the page you requested could not be found",
        ];

        if body_indicators
            .iter()
            .any(|&indicator| lower.contains(indicator))
        {
            error_count += 1;
        }

        // Only flag as error if we have at least 2 strong indicators
        return error_count >= 2;
    }

    // For 3xx status codes, not an error page (redirects)
    false
}

/// Check if HTML has valid page metadata indicating it's a real page
fn has_valid_page_metadata(html: &str) -> bool {
    let valid_patterns = [
        "<meta property=\"og:type\"",      // OpenGraph type
        "<meta property=\"og:title\"",     // OpenGraph title
        "<meta name=\"description\"",      // Meta description
        "application/ld+json",             // Structured data
        "<meta property=\"twitter:card\"", // Twitter card
    ];

    // If page has 2+ valid metadata patterns, it's likely a real page
    let metadata_count = valid_patterns
        .iter()
        .filter(|&&pattern| html.contains(pattern))
        .count();

    metadata_count >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_react_app() {
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
        assert_eq!(
            detect_engine_needed("https://example.com", html),
            EngineType::Browser
        );
    }

    #[test]
    fn test_detect_vue_app() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head></head>
            <body>
                <div id="app" data-v-123></div>
            </body>
            </html>
        "#;
        assert_eq!(
            detect_engine_needed("https://example.com", html),
            EngineType::Browser
        );
    }

    #[test]
    fn test_detect_minimal_html() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>App</title></head>
            <body>
                <div id="root"></div>
            </body>
            </html>
        "#;
        assert_eq!(
            detect_engine_needed("https://example.com", html),
            EngineType::Browser
        );
    }

    #[test]
    fn test_detect_regular_html() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Regular Page</title></head>
            <body>
                <h1>Welcome</h1>
                <p>This is a regular HTML page with plenty of content that is not a SPA.</p>
                <p>It has multiple paragraphs and elements.</p>
            </body>
            </html>
        "#;
        assert_eq!(
            detect_engine_needed("https://example.com", html),
            EngineType::Http
        );
    }

    #[test]
    fn test_validate_empty_content() {
        let result = RawScrapeResult {
            url: "https://example.com".to_string(),
            status_code: 200,
            content_type: Some("text/html".to_string()),
            html: "<html><body>Test</body></html>".to_string(),
            headers: vec![],
        };

        let markdown = "Short"; // Too short, < 100 chars

        let validation = validate_scrape_quality(&result, markdown);
        assert!(validation.is_err());
        assert!(matches!(
            validation.unwrap_err(),
            crate::error::ScrapeError::EmptyContent(_)
        ));
    }

    #[test]
    fn test_validate_error_page_by_status() {
        let result = RawScrapeResult {
            url: "https://example.com".to_string(),
            status_code: 404,
            content_type: Some("text/html".to_string()),
            html: "<html><body><h1>Not Found</h1></body></html>".to_string(),
            headers: vec![],
        };

        let markdown = "# Not Found\n\nThis is a longer markdown content that meets the minimum length requirement but is still an error page.";

        let validation = validate_scrape_quality(&result, markdown);
        assert!(validation.is_err());
        assert!(matches!(
            validation.unwrap_err(),
            crate::error::ScrapeError::ErrorPage(_)
        ));
    }

    #[test]
    fn test_validate_error_page_by_content() {
        let result = RawScrapeResult {
            url: "https://example.com".to_string(),
            status_code: 200,
            content_type: Some("text/html".to_string()),
            html: "<html><body><h1>404 Not Found</h1><p>The page you are looking for does not exist.</p></body></html>".to_string(),
            headers: vec![],
        };

        let markdown =
            "# 404 Not Found\n\nThe page you are looking for does not exist. This is long enough to pass the length check.";

        let validation = validate_scrape_quality(&result, markdown);
        assert!(validation.is_err());
        assert!(matches!(
            validation.unwrap_err(),
            crate::error::ScrapeError::ErrorPage(_)
        ));
    }

    #[test]
    fn test_validate_good_content() {
        let result = RawScrapeResult {
            url: "https://example.com".to_string(),
            status_code: 200,
            content_type: Some("text/html".to_string()),
            html: r#"
                <html>
                <head><title>Good Page</title></head>
                <body>
                    <h1>Welcome to our site</h1>
                    <p>This is a well-formed page with plenty of content.</p>
                    <p>It has multiple paragraphs and meaningful information.</p>
                    <p>The content density is reasonable.</p>
                </body>
                </html>
            "#
            .to_string(),
            headers: vec![],
        };

        let markdown = r#"
# Welcome to our site

This is a well-formed page with plenty of content.

It has multiple paragraphs and meaningful information.

The content density is reasonable.
        "#;

        let validation = validate_scrape_quality(&result, markdown);
        assert!(validation.is_ok());
    }

    #[test]
    fn test_validate_low_quality_content() {
        // Create HTML with lots of non-text content (comments, styles, etc.) to make density low
        let css_comments = "/* ".repeat(500); // 1000 chars of CSS comments
        let html_comments = "<!--".repeat(500); // 2000 chars of HTML comments
        let html_parts = vec![
            r#"<html><head><style>"#,
            &css_comments,
            r#"*/ body { margin: 0; } </style></head><body>"#,
            "T", // Just 1 char of actual text
            &html_comments,
            r#"--> </body></html>"#,
        ];
        let html = html_parts.join("");

        // Debug: check the density
        let density = calculate_content_density(&html);
        let html_len = html.len();
        eprintln!(
            "Content density: {} (html len: {}, text len: ~{})",
            density,
            html_len,
            (density * html_len as f64) as usize
        );

        let result = RawScrapeResult {
            url: "https://example.com".to_string(),
            status_code: 500,
            content_type: Some("text/html".to_string()),
            html,
            headers: vec![],
        };

        // Markdown is longer than 100 chars to pass length check
        let markdown = "This markdown is long enough to pass the minimum length requirement of 100 characters but still represents very low density content.";

        let validation = validate_scrape_quality(&result, markdown);
        if let Err(ref e) = validation {
            eprintln!("Validation error: {:?}", e);
        }
        assert!(validation.is_err(), "Expected validation to fail");

        // Check what error we got
        match validation.unwrap_err() {
            crate::error::ScrapeError::LowQuality(_) => {
                // This is what we expect
            }
            crate::error::ScrapeError::ErrorPage(_) => {
                // This is acceptable too since status is 500
            }
            other => {
                panic!("Expected LowQuality or ErrorPage, got: {:?}", other);
            }
        }
    }

    #[test]
    fn test_calculate_content_density() {
        let html = "<html><body>Test</body></html>";
        let density = calculate_content_density(html);
        // "Test" is 4 chars, HTML is 30 chars, so density should be ~0.133
        assert!(density > 0.1 && density < 0.2);

        let empty_html = "";
        let empty_density = calculate_content_density(empty_html);
        assert_eq!(empty_density, 0.0);
    }

    #[test]
    fn test_is_likely_error_page() {
        // Error status codes are always error pages
        assert!(is_likely_error_page("Some content", 404));
        assert!(is_likely_error_page("Some content", 500));

        // Pages with valid metadata should NOT be flagged as errors
        let valid_page_with_metadata = r#"
            <html>
            <head>
                <meta property="og:type" content="website">
                <meta property="og:title" content="IMDb">
                <meta name="description" content="Movie database">
            </head>
            <body>Error occurred in JavaScript code</body>
            </html>
        "#;
        assert!(!is_likely_error_page(valid_page_with_metadata, 200));

        // Page with error in title AND heading should be flagged (multiple indicators, no valid metadata)
        let error_in_title = "<html><head><title>404 Not Found</title></head><body><h1>404 Not Found</h1></body></html>";
        assert!(is_likely_error_page(error_in_title, 200));

        // Page with error in h1 and body should be flagged (multiple indicators)
        let error_in_heading = "<html><body><h1>404 Not Found</h1><p>The page you are looking for does not exist</p></body></html>";
        assert!(is_likely_error_page(error_in_heading, 200));

        // Page with just "error" in body text (only 1 indicator) should NOT be flagged
        let normal_page =
            "<html><body><p>Welcome to our site. Error handling is important.</p></body></html>";
        assert!(!is_likely_error_page(normal_page, 200));

        // IMDb-like page with "error occurred" in JavaScript should NOT be flagged
        let imdb_like = r#"
            <html>
            <head>
                <meta property="og:type" content="website">
                <meta name="description" content="IMDb content">
                <title>IMDb: Ratings, Reviews, and Where to Watch</title>
            </head>
            <body>
                <script>
                    if (error occurred) { console.log("error occurred"); }
                </script>
                <h1>Welcome to IMDb</h1>
            </body>
            </html>
        "#;
        assert!(!is_likely_error_page(imdb_like, 200));
    }

    #[test]
    fn test_has_valid_page_metadata() {
        let with_metadata = r#"
            <meta property="og:type" content="website">
            <meta property="og:title" content="Test">
            <meta name="description" content="Test page">
        "#;
        assert!(has_valid_page_metadata(with_metadata));

        let with_one_metadata = r#"
            <meta name="description" content="Test page">
        "#;
        assert!(!has_valid_page_metadata(with_one_metadata)); // Need at least 2

        let no_metadata = "<html><body>Test</body></html>";
        assert!(!has_valid_page_metadata(no_metadata));
    }
}
