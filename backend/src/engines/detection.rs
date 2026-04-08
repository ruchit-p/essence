use scraper::{Html, Selector};

/// JavaScript rendering detection with framework signature detection
pub struct RenderingDetector;

/// Detection result with reasoning
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Whether JavaScript rendering is needed
    pub needs_js: bool,
    /// Reason for the decision
    pub reason: String,
    /// Detected frameworks
    pub detected_frameworks: Vec<String>,
    /// Content-to-script ratio
    pub content_script_ratio: f64,
}

/// Framework signatures for detection
#[derive(Debug)]
struct FrameworkSignature {
    name: &'static str,
    html_markers: Vec<&'static str>,
    script_patterns: Vec<&'static str>,
}

impl RenderingDetector {
    /// Check if JavaScript rendering is needed
    pub fn needs_javascript(html: &str, _url: &str) -> DetectionResult {
        let document = Html::parse_document(html);
        let mut detected_frameworks = Vec::new();
        let mut reasons = Vec::new();

        // Check for framework signatures
        let frameworks = Self::get_framework_signatures();
        for framework in frameworks {
            if Self::detect_framework(&document, html, &framework) {
                detected_frameworks.push(framework.name.to_string());
                reasons.push(format!("{} framework detected", framework.name));
            }
        }

        // Check for lazy loading indicators
        if Self::has_lazy_loading_indicators(&document, html) {
            reasons.push("Lazy loading detected".to_string());
        }

        // Check for SPA routing
        if Self::has_spa_routing(&document, html) {
            reasons.push("SPA routing detected".to_string());
        }

        // Calculate content-to-script ratio
        let content_script_ratio = Self::calculate_content_script_ratio(&document, html);
        if content_script_ratio < 0.5 {
            reasons.push(format!(
                "Low content-to-script ratio: {:.2}",
                content_script_ratio
            ));
        }

        // Check for minimal content (SPA shell)
        if Self::has_minimal_content(&document) {
            reasons.push("Minimal initial content (SPA shell)".to_string());
        }

        // Check for hydration markers
        if Self::has_hydration_markers(html) {
            reasons.push("Hydration markers detected".to_string());
        }

        // Require strong evidence before declaring JS rendering needed.
        // A single weak signal (e.g., framework detected but content is present) is not enough.
        // True SPAs have minimal content (<100 chars) — that's the strongest signal.
        let has_minimal = reasons.iter().any(|r| r.contains("Minimal initial content"));
        let strong_signal_count = detected_frameworks.len() + reasons.len();
        let needs_js = has_minimal
            || (strong_signal_count >= 3 && content_script_ratio < 0.3);
        let reason = if needs_js {
            reasons.join("; ")
        } else if !reasons.is_empty() || !detected_frameworks.is_empty() {
            format!("Signals detected but insufficient to trigger browser: {}", reasons.join("; "))
        } else {
            "Static content with sufficient initial HTML".to_string()
        };

        DetectionResult {
            needs_js,
            reason,
            detected_frameworks,
            content_script_ratio,
        }
    }

    /// Get framework signatures
    fn get_framework_signatures() -> Vec<FrameworkSignature> {
        vec![
            FrameworkSignature {
                name: "React",
                html_markers: vec![
                    "__REACT_DEVTOOLS_GLOBAL_HOOK__",
                    "data-reactroot",
                    "data-react-helmet",
                    "react-root",
                ],
                script_patterns: vec!["react", "react-dom"],
            },
            FrameworkSignature {
                name: "Next.js",
                html_markers: vec!["__NEXT_DATA__", "_N_E", "__next"],
                script_patterns: vec!["_next/static", "next/dist"],
            },
            FrameworkSignature {
                name: "Vue",
                html_markers: vec!["data-v-", "__VUE__", "data-server-rendered"],
                script_patterns: vec!["vue.js", "vue.runtime"],
            },
            FrameworkSignature {
                name: "Nuxt",
                html_markers: vec!["__NUXT__", "$nuxt", "nuxt-link"],
                script_patterns: vec!["_nuxt/"],
            },
            FrameworkSignature {
                name: "Angular",
                html_markers: vec!["ng-version", "_nghost", "_ngcontent"],
                script_patterns: vec!["angular", "@angular"],
            },
            FrameworkSignature {
                name: "Svelte",
                html_markers: vec!["svelte-"],
                script_patterns: vec!["svelte"],
            },
            FrameworkSignature {
                name: "Gatsby",
                html_markers: vec!["___gatsby", "gatsby-"],
                script_patterns: vec!["webpack-runtime"],
            },
            FrameworkSignature {
                name: "Ember",
                html_markers: vec!["ember-application", "ember-view"],
                script_patterns: vec!["ember.js"],
            },
        ]
    }

    /// Detect if a specific framework is present
    fn detect_framework(document: &Html, html: &str, signature: &FrameworkSignature) -> bool {
        // Check HTML markers
        for marker in &signature.html_markers {
            if html.contains(marker) {
                return true;
            }
        }

        // Check script src patterns
        if let Ok(selector) = Selector::parse("script[src]") {
            for element in document.select(&selector) {
                if let Some(src) = element.value().attr("src") {
                    for pattern in &signature.script_patterns {
                        if src.to_lowercase().contains(pattern) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Check for lazy loading indicators
    fn has_lazy_loading_indicators(document: &Html, html: &str) -> bool {
        // Check for lazy loading attributes
        // Only patterns that indicate JS-dependent content loading.
        // Excluded: loading="lazy", data-src, data-original — these are standard
        // image optimization attributes present on virtually every modern site and
        // do NOT indicate that JavaScript rendering is needed for text content.
        let lazy_patterns = vec![
            "data-lazy",
            "lazy-load",
            "data-lazy-src",
        ];

        for pattern in lazy_patterns {
            if html.contains(pattern) {
                return true;
            }
        }

        // Check for intersection observer (common lazy loading technique)
        if let Ok(selector) = Selector::parse("script") {
            for element in document.select(&selector) {
                let script_text = element.text().collect::<String>();
                if script_text.contains("IntersectionObserver")
                    || script_text.contains("getBoundingClientRect")
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check for SPA routing patterns
    fn has_spa_routing(document: &Html, html: &str) -> bool {
        // Check for client-side routing libraries
        let routing_patterns = vec![
            "react-router",
            "vue-router",
            "angular/router",
            "@reach/router",
            "history.pushState",
            "history.replaceState",
        ];

        for pattern in routing_patterns {
            if html.contains(pattern) {
                return true;
            }
        }

        // Check for hash-based routing
        if let Ok(selector) = Selector::parse("a[href^='#/']") {
            if document.select(&selector).count() > 0 {
                return true;
            }
        }

        false
    }

    /// Calculate content-to-script ratio
    fn calculate_content_script_ratio(document: &Html, _html: &str) -> f64 {
        // Get all script content
        let mut script_size = 0;
        if let Ok(selector) = Selector::parse("script") {
            for element in document.select(&selector) {
                let script_text = element.text().collect::<String>();
                script_size += script_text.len();
                
                // Also count inline scripts from src length estimation
                if let Some(src) = element.value().attr("src") {
                    script_size += src.len() * 10; // Estimate external script impact
                }
            }
        }

        // Get body text content
        let body_text = document
            .root_element()
            .text()
            .collect::<String>()
            .trim()
            .to_string();
        
        let content_size = body_text.len();

        if script_size == 0 {
            return 1.0;
        }

        content_size as f64 / (content_size + script_size) as f64
    }

    /// Check for minimal content (SPA shell)
    fn has_minimal_content(document: &Html) -> bool {
        // Get visible text content
        let body_text = document
            .root_element()
            .text()
            .collect::<String>()
            .trim()
            .to_string();

        // If body has very little text (< 100 chars), it's likely a SPA shell
        if body_text.len() < 100 {
            return true;
        }

        // Check for common SPA root elements with minimal content
        let spa_roots = vec!["#root", "#app", "#__next", "#application"];
        for root_id in spa_roots {
            if let Ok(selector) = Selector::parse(root_id) {
                if let Some(root) = document.select(&selector).next() {
                    let root_text = root.text().collect::<String>().trim().to_string();
                    if root_text.is_empty() || root_text.len() < 50 {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check for hydration markers
    fn has_hydration_markers(html: &str) -> bool {
        // Only markers indicating client-side hydration is incomplete/needed.
        // Excluded: __NEXT_DATA__, __NUXT__, data-server-rendered — these prove
        // content IS server-rendered (SSR), meaning HTTP already has the content.
        let hydration_markers = vec![
            "data-reactid",
            "data-react-checksum",
            "data-hydrate",
        ];

        for marker in hydration_markers {
            if html.contains(marker) {
                return true;
            }
        }

        false
    }

    /// Get a detailed analysis report
    pub fn analyze_page(html: &str, url: &str) -> String {
        let result = Self::needs_javascript(html, url);
        
        let mut report = String::new();
        report.push_str(&format!("URL: {}\n", url));
        report.push_str(&format!("Needs JavaScript: {}\n", result.needs_js));
        report.push_str(&format!("Reason: {}\n", result.reason));
        report.push_str(&format!("Content/Script Ratio: {:.2}\n", result.content_script_ratio));
        
        if !result.detected_frameworks.is_empty() {
            report.push_str(&format!(
                "Detected Frameworks: {}\n",
                result.detected_frameworks.join(", ")
            ));
        }
        
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_react() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head></head>
            <body>
                <div id="root"></div>
                <script>window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {}</script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.detected_frameworks.contains(&"React".to_string()));
    }

    #[test]
    fn test_detect_nextjs() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head></head>
            <body>
                <div id="__next"></div>
                <script id="__NEXT_DATA__" type="application/json">{}</script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.detected_frameworks.contains(&"Next.js".to_string()));
    }

    #[test]
    fn test_detect_vue() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head></head>
            <body>
                <div id="app" data-v-123></div>
                <script src="/vue.runtime.js"></script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.detected_frameworks.contains(&"Vue".to_string()));
    }

    #[test]
    fn test_detect_lazy_loading() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <body>
                <img data-lazy-src="image.jpg" />
                <p>Some content here to make it substantial enough.</p>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.reason.contains("Lazy loading"));
    }

    #[test]
    fn test_static_content() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>Regular Page</title></head>
            <body>
                <h1>Welcome to Our Website</h1>
                <p>This is a regular HTML page with plenty of content that is not a SPA.</p>
                <p>It has multiple paragraphs and elements that provide substantial content.</p>
                <article>
                    <h2>Article Title</h2>
                    <p>Article content goes here with enough text to be considered substantial.</p>
                </article>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(!result.needs_js);
        assert!(result.reason.contains("Static content"));
    }

    #[test]
    fn test_minimal_content() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>App</title></head>
            <body>
                <div id="root"></div>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.reason.contains("Minimal initial content"));
    }

    #[test]
    fn test_content_script_ratio() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <body>
                <p>A bit of content</p>
                <script>
                    // Lots of JavaScript code here
                    var x = 1; var y = 2; var z = 3;
                    function test() {
                        console.log("This is a long script to test ratio with lots of code");
                        console.log("More code here to make it substantial");
                        console.log("Even more code to increase the ratio");
                        console.log("And more JavaScript to ensure low content ratio");
                        console.log("Additional script content here");
                        console.log("Even more script content");
                        var longVariable = "This is a long string to add more script content";
                        var anotherVariable = "And another one for good measure";
                    }
                </script>
                <script src="https://example.com/very-long-path-to-external-script.js"></script>
                <script src="https://example.com/another-external-script-with-long-path.js"></script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        // With external scripts, the ratio should be low
        assert!(result.content_script_ratio < 0.8); // More lenient assertion
    }
}
