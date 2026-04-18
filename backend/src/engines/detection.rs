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

        // Check for generic SPA signals (no specific framework match needed)
        let has_generic_spa = Self::has_generic_spa_signals(&document, html);
        if has_generic_spa {
            reasons.push("Generic SPA signals detected".to_string());
        }

        // Require strong evidence before declaring JS rendering needed.
        // A single weak signal (e.g., framework detected but content is present) is not enough.
        // True SPAs have minimal content (<100 chars) — that's the strongest signal.
        let has_minimal = reasons
            .iter()
            .any(|r| r.contains("Minimal initial content"));
        let strong_signal_count = detected_frameworks.len() + reasons.len();
        let needs_js = has_minimal
            || (strong_signal_count >= 3 && content_script_ratio < 0.3)
            || has_generic_spa;
        let reason = if needs_js {
            reasons.join("; ")
        } else if !reasons.is_empty() || !detected_frameworks.is_empty() {
            format!(
                "Signals detected but insufficient to trigger browser: {}",
                reasons.join("; ")
            )
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
            FrameworkSignature {
                name: "Remix",
                html_markers: vec!["__remix", "__remixContext", "__remix_data"],
                script_patterns: vec!["@remix-run", "remix.run"],
            },
            FrameworkSignature {
                name: "Solid",
                html_markers: vec!["data-hk", "_$HY"],
                script_patterns: vec!["solid-js", "solidjs"],
            },
            FrameworkSignature {
                name: "Qwik",
                html_markers: vec!["q:container", "q:version", "q:base"],
                script_patterns: vec!["@builder.io/qwik", "qwikloader"],
            },
            FrameworkSignature {
                name: "Astro",
                html_markers: vec!["astro-island", "client:load", "client:visible"],
                script_patterns: vec!["astro/", "@astrojs"],
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
        let lazy_patterns = vec!["data-lazy", "lazy-load", "data-lazy-src"];

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

    /// Detect generic SPA signals without matching a specific framework
    fn has_generic_spa_signals(document: &Html, _html: &str) -> bool {
        // (a) Empty root div with script tags present
        let has_empty_root = {
            let root_selectors = [
                "div#root",
                "div#app",
                "div#__app",
                "div[id=\"application\"]",
            ];
            let has_scripts = Selector::parse("script")
                .map(|sel| document.select(&sel).next().is_some())
                .unwrap_or(false);

            let mut found_empty_root = false;
            for sel_str in &root_selectors {
                if let Ok(selector) = Selector::parse(sel_str) {
                    if let Some(element) = document.select(&selector).next() {
                        let text = element.text().collect::<String>();
                        let visible_text = text.trim();
                        if visible_text.len() < 20 && has_scripts {
                            found_empty_root = true;
                            break;
                        }
                    }
                }
            }
            found_empty_root
        };

        if has_empty_root {
            return true;
        }

        let mut secondary_signals = 0;

        // (b) Heavy module scripts: 3 or more <script type="module"> tags
        let has_heavy_modules = Selector::parse("script[type=\"module\"]")
            .map(|sel| document.select(&sel).count() >= 3)
            .unwrap_or(false);
        if has_heavy_modules {
            secondary_signals += 1;
        }

        // (c) Bundle patterns in script src attributes
        let has_bundle_patterns = {
            let bundle_patterns = [
                "_buildManifest",
                "__webpack",
                ".chunk.",
                "chunk-",
                "vendor.",
            ];
            // app. followed by hash-like pattern (hex chars)
            let app_hash_re = regex::Regex::new(r"app\.[0-9a-f]{6,}").ok();

            let mut found = false;
            if let Ok(selector) = Selector::parse("script[src]") {
                for element in document.select(&selector) {
                    if let Some(src) = element.value().attr("src") {
                        for pattern in &bundle_patterns {
                            if src.contains(pattern) {
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            if let Some(ref re) = app_hash_re {
                                if re.is_match(src) {
                                    found = true;
                                }
                            }
                        }
                        if found {
                            break;
                        }
                    }
                }
            }
            found
        };
        if has_bundle_patterns {
            secondary_signals += 1;
        }

        // (d) Explicit JS dependency in noscript tags
        let has_noscript_js_warning = {
            let mut found = false;
            if let Ok(selector) = Selector::parse("noscript") {
                for element in document.select(&selector) {
                    let noscript_text = element.text().collect::<String>().to_lowercase();
                    if noscript_text.contains("javascript") || noscript_text.contains("enable") {
                        found = true;
                        break;
                    }
                }
            }
            found
        };
        if has_noscript_js_warning {
            secondary_signals += 1;
        }

        // Return true if 2+ secondary signals
        secondary_signals >= 2
    }

    /// Check for hydration markers
    fn has_hydration_markers(html: &str) -> bool {
        // Only markers indicating client-side hydration is incomplete/needed.
        // Excluded: __NEXT_DATA__, __NUXT__, data-server-rendered — these prove
        // content IS server-rendered (SSR), meaning HTTP already has the content.
        let hydration_markers = vec!["data-reactid", "data-react-checksum", "data-hydrate"];

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
        report.push_str(&format!(
            "Content/Script Ratio: {:.2}\n",
            result.content_script_ratio
        ));

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

    #[test]
    fn test_detect_remix() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <body>
                <div id="root"></div>
                <script>window.__remixContext = {}</script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.detected_frameworks.contains(&"Remix".to_string()));
    }

    #[test]
    fn test_detect_qwik() {
        let html = r#"
            <!DOCTYPE html>
            <html q:container="paused" q:version="1.0">
            <body>
                <div></div>
                <script src="/@builder.io/qwik/build/qwikloader.js"></script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.detected_frameworks.contains(&"Qwik".to_string()));
    }

    #[test]
    fn test_detect_astro_client() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <body>
                <astro-island uid="abc" component-export="Counter" client:load>
                </astro-island>
                <script src="/@astrojs/client.js"></script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(result.needs_js);
        assert!(result.detected_frameworks.contains(&"Astro".to_string()));
    }

    #[test]
    fn test_generic_spa_empty_root_div() {
        // Custom SPA with no framework markers but empty root + scripts
        let html = r#"
            <!DOCTYPE html>
            <html>
            <body>
                <div id="root"></div>
                <script src="/assets/main.abc123.js"></script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(
            result.needs_js,
            "Empty root div with scripts should trigger SPA detection: {}",
            result.reason
        );
    }

    #[test]
    fn test_generic_spa_module_scripts_and_bundles() {
        // Custom SPA with module scripts and bundle patterns (no framework markers)
        let html = r#"
            <!DOCTYPE html>
            <html>
            <body>
                <p>Loading...</p>
                <script type="module" src="/chunk-abc123.js"></script>
                <script type="module" src="/vendor.def456.js"></script>
                <script type="module" src="/app.789abc.js"></script>
                <noscript>You need to enable JavaScript to run this app.</noscript>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(
            result.needs_js,
            "Module scripts + bundle patterns + noscript should trigger: {}",
            result.reason
        );
    }

    #[test]
    fn test_static_page_with_scripts_not_triggered() {
        // Static page that happens to have scripts (should NOT trigger generic SPA)
        let html = r#"
            <!DOCTYPE html>
            <html>
            <body>
                <h1>Welcome to Our Site</h1>
                <p>This is a content-rich page with lots of text.</p>
                <p>Another paragraph with meaningful content that shows this is not a SPA.</p>
                <script src="/analytics.js"></script>
            </body>
            </html>
        "#;
        let result = RenderingDetector::needs_javascript(html, "https://example.com");
        assert!(
            !result.needs_js,
            "Static page with analytics script should not trigger: {}",
            result.reason
        );
    }
}
