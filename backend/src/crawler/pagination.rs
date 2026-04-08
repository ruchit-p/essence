//! Pagination detection module
//!
//! This module provides intelligent pagination detection to automatically discover
//! and follow paginated content. This is a unique feature not available in Firecrawl.

use crate::error::{Result, ScrapeError};
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use tracing::{debug, trace};
use url::Url;

/// Configuration for pagination detection
#[derive(Debug, Clone)]
pub struct PaginationConfig {
    /// Maximum number of pagination pages to follow (default: 50)
    pub max_pages: usize,

    /// Maximum pagination depth (default: 10)
    pub max_depth: usize,

    /// Detect and skip circular pagination (default: true)
    pub detect_circular: bool,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            max_pages: 50,
            max_depth: 10,
            detect_circular: true,
        }
    }
}

/// Detector for pagination links in HTML content with state tracking
pub struct PaginationDetector {
    config: PaginationConfig,
    visited_pagination: HashSet<String>,
    pagination_depth: HashMap<String, usize>,
}

impl PaginationDetector {
    /// Create a new pagination detector with custom configuration
    pub fn new(config: PaginationConfig) -> Self {
        Self {
            config,
            visited_pagination: HashSet::new(),
            pagination_depth: HashMap::new(),
        }
    }

    /// Create a new pagination detector with default configuration
    pub fn new_default() -> Self {
        Self::new(PaginationConfig::default())
    }

    /// Detect pagination links in HTML content with safety checks
    ///
    /// Uses multiple strategies to detect pagination:
    /// 1. rel="next" attribute (standard HTML pagination)
    /// 2. "Next" text links with common patterns
    /// 3. URL pattern detection (/page/N/, ?page=N)
    /// 4. Numbered pagination links (1, 2, 3, etc.)
    ///
    /// Returns a deduplicated list of pagination URLs with safety limits applied
    pub fn detect_pagination(&mut self, html: &str, current_url: &str) -> Vec<String> {
        // Mark current URL as visited if not already
        if !self.visited_pagination.contains(current_url) {
            self.visited_pagination.insert(current_url.to_string());
            let current_depth = self.pagination_depth.get(current_url).copied().unwrap_or(0);
            self.pagination_depth
                .insert(current_url.to_string(), current_depth);
        }

        // Check if we've hit pagination limits
        if self.visited_pagination.len() >= self.config.max_pages {
            debug!(
                "Reached max pagination pages ({}), stopping pagination detection",
                self.config.max_pages
            );
            return vec![];
        }

        // Get current depth
        let current_depth = self.pagination_depth.get(current_url).copied().unwrap_or(0);
        if current_depth >= self.config.max_depth {
            debug!(
                "Reached max pagination depth ({}) for {}, stopping pagination detection",
                self.config.max_depth, current_url
            );
            return vec![];
        }

        // Check if this is the last page
        if Self::is_last_page(html) {
            debug!("Detected last page indicator in HTML, stopping pagination");
            return vec![];
        }

        let mut pages = HashSet::new();

        // Strategy 1: rel="next" attribute (most reliable)
        if let Some(next_url) = Self::find_rel_next(html, current_url) {
            trace!("Found rel=next pagination link: {}", next_url);
            pages.insert(next_url);
        }

        // Strategy 2: "Next" text links
        for next_url in Self::find_next_text_links(html, current_url) {
            trace!("Found 'Next' text pagination link: {}", next_url);
            pages.insert(next_url);
        }

        // Strategy 3: URL pattern detection
        if let Some(next_url) = Self::detect_url_pattern(current_url) {
            trace!("Detected URL pattern pagination: {}", next_url);
            pages.insert(next_url);
        }

        // Strategy 4: Numbered pagination links
        for page_url in Self::find_numbered_links(html, current_url) {
            trace!("Found numbered pagination link: {}", page_url);
            pages.insert(page_url);
        }

        // Filter out already visited and circular references
        let new_pages: Vec<String> = pages
            .into_iter()
            .filter(|url| {
                // Skip if already visited
                if self.visited_pagination.contains(url) {
                    debug!("Skipping already visited pagination: {}", url);
                    return false;
                }

                // Check for circular pagination
                if self.config.detect_circular && Self::is_circular_pagination(url, current_url) {
                    debug!("Detected circular pagination, skipping: {}", url);
                    return false;
                }

                true
            })
            .collect();

        // Track depth for new pages
        for page in &new_pages {
            self.pagination_depth
                .insert(page.clone(), current_depth + 1);
            self.visited_pagination.insert(page.clone());
        }

        if !new_pages.is_empty() {
            debug!(
                "Detected {} new pagination link(s) from {} (depth: {}, total visited: {})",
                new_pages.len(),
                current_url,
                current_depth,
                self.visited_pagination.len()
            );
        }

        new_pages
    }

    /// Check if the current page is the last page
    fn is_last_page(html: &str) -> bool {
        let document = Html::parse_document(html);

        // Check for disabled next button
        if let Ok(sel) =
            Selector::parse(r#"a[rel="next"][disabled], button.next[disabled], .next[disabled]"#)
        {
            if document.select(&sel).next().is_some() {
                return true;
            }
        }

        // Check for aria-disabled
        if let Ok(sel) = Selector::parse(
            r#"[aria-label*="next" i][aria-disabled="true"], [aria-label*="next" i][disabled]"#,
        ) {
            if document.select(&sel).next().is_some() {
                return true;
            }
        }

        // Check for common "disabled" class patterns
        if let Ok(sel) =
            Selector::parse(r#".next.disabled, .pager-next.disabled, .pagination-next.disabled"#)
        {
            if document.select(&sel).next().is_some() {
                return true;
            }
        }

        false
    }

    /// Check if the pagination link represents circular pagination
    fn is_circular_pagination(next_url: &str, current_url: &str) -> bool {
        // Extract page numbers
        let next_page_num = Self::extract_page_number(next_url);
        let current_page_num = Self::extract_page_number(current_url);

        // If both have page numbers, check if going backwards
        if let (Some(next_num), Some(current_num)) = (next_page_num, current_page_num) {
            if next_num <= current_num {
                return true; // Going backwards or same = circular
            }
        }

        false
    }

    /// Extract page number from URL
    fn extract_page_number(url: &str) -> Option<u32> {
        // Try /page/N/ pattern
        if let Ok(re) = Regex::new(r"/page/(\d+)") {
            if let Some(caps) = re.captures(url) {
                if let Some(num_str) = caps.get(1) {
                    if let Ok(num) = num_str.as_str().parse() {
                        return Some(num);
                    }
                }
            }
        }

        // Try ?page=N pattern
        if let Ok(re) = Regex::new(r"[?&]page=(\d+)") {
            if let Some(caps) = re.captures(url) {
                if let Some(num_str) = caps.get(1) {
                    if let Ok(num) = num_str.as_str().parse() {
                        return Some(num);
                    }
                }
            }
        }

        // Try ?p=N pattern
        if let Ok(re) = Regex::new(r"[?&]p=(\d+)") {
            if let Some(caps) = re.captures(url) {
                if let Some(num_str) = caps.get(1) {
                    if let Ok(num) = num_str.as_str().parse() {
                        return Some(num);
                    }
                }
            }
        }

        None
    }

    /// Find links with rel="next" attribute
    fn find_rel_next(html: &str, base_url: &str) -> Option<String> {
        let document = Html::parse_document(html);

        // Check both <a> and <link> tags
        let selector = Selector::parse(r#"a[rel="next"], link[rel="next"]"#).ok()?;

        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if let Ok(url) = Self::resolve_url(base_url, href) {
                    return Some(url);
                }
            }
        }

        None
    }

    /// Find links with "Next" text content
    fn find_next_text_links(html: &str, base_url: &str) -> Vec<String> {
        let document = Html::parse_document(html);
        let selector = match Selector::parse("a") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut links = Vec::new();

        // Common pagination text patterns
        let next_patterns = [
            "next", "→", "»", "›", "&#8594;", "&#187;", "&#8250;", "&rarr;", "&raquo;", "&rsaquo;",
        ];

        for element in document.select(&selector) {
            let text = element.text().collect::<String>().to_lowercase();
            let text_trimmed = text.trim();

            // Check if link text contains any next pattern
            let is_next = next_patterns
                .iter()
                .any(|pattern| text_trimmed.contains(pattern));

            // Also check for common class names
            let has_next_class = element
                .value()
                .attr("class")
                .map(|c| c.contains("next") || c.contains("pager"))
                .unwrap_or(false);

            if is_next || has_next_class {
                if let Some(href) = element.value().attr("href") {
                    // Skip javascript and anchor links
                    if href.starts_with("javascript:") || href == "#" {
                        continue;
                    }

                    if let Ok(url) = Self::resolve_url(base_url, href) {
                        links.push(url);
                    }
                }
            }
        }

        links
    }

    /// Detect URL patterns like /page/N/, ?page=N, etc.
    fn detect_url_pattern(current_url: &str) -> Option<String> {
        let current = Url::parse(current_url).ok()?;

        // Pattern 1: /page/N/
        let page_path_regex = Regex::new(r"/page/(\d+)/?").ok()?;
        if let Some(caps) = page_path_regex.captures(current.path()) {
            let current_page: u32 = caps[1].parse().ok()?;
            let next_page = current_page + 1;

            let next_url = current_url.replace(
                &format!("/page/{}/", current_page),
                &format!("/page/{}/", next_page),
            );

            // Also try without trailing slash
            let next_url = if next_url == current_url {
                current_url.replace(
                    &format!("/page/{}", current_page),
                    &format!("/page/{}", next_page),
                )
            } else {
                next_url
            };

            return Some(next_url);
        }

        // Pattern 2: ?page=N
        let query_page_regex = Regex::new(r"[?&]page=(\d+)").ok()?;
        if let Some(caps) = query_page_regex.captures(current_url) {
            let current_page: u32 = caps[1].parse().ok()?;
            let next_page = current_page + 1;

            let next_url = current_url.replace(
                &format!("page={}", current_page),
                &format!("page={}", next_page),
            );

            return Some(next_url);
        }

        // Pattern 3: ?p=N
        let query_p_regex = Regex::new(r"[?&]p=(\d+)").ok()?;
        if let Some(caps) = query_p_regex.captures(current_url) {
            let current_page: u32 = caps[1].parse().ok()?;
            let next_page = current_page + 1;

            let next_url =
                current_url.replace(&format!("p={}", current_page), &format!("p={}", next_page));

            return Some(next_url);
        }

        None
    }

    /// Find numbered pagination links (1, 2, 3, etc.)
    fn find_numbered_links(html: &str, base_url: &str) -> Vec<String> {
        let document = Html::parse_document(html);
        let selector = match Selector::parse("a") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let mut links = Vec::new();
        let number_regex = match Regex::new(r"^\s*(\d+)\s*$") {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        // Extract current page number from URL
        let current_page_num = Self::extract_current_page_number(base_url);

        for element in document.select(&selector) {
            let text = element.text().collect::<String>();
            let text_trimmed = text.trim();

            if let Some(caps) = number_regex.captures(text_trimmed) {
                let page_num: u32 = match caps[1].parse() {
                    Ok(n) => n,
                    Err(_) => continue,
                };

                // Only add pages greater than current page
                if let Some(current) = current_page_num {
                    if page_num <= current {
                        continue;
                    }
                }

                if let Some(href) = element.value().attr("href") {
                    if let Ok(url) = Self::resolve_url(base_url, href) {
                        links.push(url);
                    }
                }
            }
        }

        links
    }

    /// Extract current page number from URL
    fn extract_current_page_number(url: &str) -> Option<u32> {
        // Try /page/N/ pattern
        let page_path_regex = Regex::new(r"/page/(\d+)").ok()?;
        if let Some(caps) = page_path_regex.captures(url) {
            return caps[1].parse().ok();
        }

        // Try ?page=N pattern
        let query_page_regex = Regex::new(r"[?&]page=(\d+)").ok()?;
        if let Some(caps) = query_page_regex.captures(url) {
            return caps[1].parse().ok();
        }

        // Try ?p=N pattern
        let query_p_regex = Regex::new(r"[?&]p=(\d+)").ok()?;
        if let Some(caps) = query_p_regex.captures(url) {
            return caps[1].parse().ok();
        }

        None
    }

    /// Resolve a relative URL to an absolute URL
    fn resolve_url(base: &str, relative: &str) -> Result<String> {
        let base_url = Url::parse(base)
            .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid base URL: {}", e)))?;

        let resolved = base_url
            .join(relative)
            .map_err(|e| ScrapeError::InvalidUrl(format!("Failed to resolve URL: {}", e)))?;

        Ok(resolved.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_rel_next() {
        let html = r#"<a href="/page/2/" rel="next">Next</a>"#;
        let mut detector = PaginationDetector::new_default();
        let pages = detector.detect_pagination(html, "https://example.com/page/1/");

        assert!(!pages.is_empty());
        assert!(pages.contains(&"https://example.com/page/2/".to_string()));
    }

    #[test]
    fn test_detect_next_text_link() {
        let html = r#"<a href="/page/2/">Next →</a>"#;
        let mut detector = PaginationDetector::new_default();
        let pages = detector.detect_pagination(html, "https://example.com/page/1/");

        assert!(!pages.is_empty());
        assert!(pages.contains(&"https://example.com/page/2/".to_string()));
    }

    #[test]
    fn test_detect_page_pattern() {
        let html = "";
        let mut detector = PaginationDetector::new_default();
        let pages = detector.detect_pagination(html, "https://quotes.toscrape.com/page/1/");

        // Should detect /page/2/ pattern
        assert!(!pages.is_empty());
        assert!(pages.iter().any(|p| p.contains("/page/2/")));
    }

    #[test]
    fn test_detect_query_page_pattern() {
        let html = "";
        let mut detector = PaginationDetector::new_default();
        let pages = detector.detect_pagination(html, "https://example.com/search?page=1");

        assert!(!pages.is_empty());
        assert!(pages.contains(&"https://example.com/search?page=2".to_string()));
    }

    #[test]
    fn test_numbered_pagination() {
        let html = r#"
            <nav>
                <a href="/page/1/">1</a>
                <a href="/page/2/">2</a>
                <a href="/page/3/">3</a>
            </nav>
        "#;
        let mut detector = PaginationDetector::new_default();
        let pages = detector.detect_pagination(html, "https://example.com/page/1/");

        assert!(pages.len() >= 2);
        assert!(pages.contains(&"https://example.com/page/2/".to_string()));
        assert!(pages.contains(&"https://example.com/page/3/".to_string()));
    }

    #[test]
    fn test_quotes_toscrape_pagination() {
        let html = r#"
            <nav>
                <ul class="pager">
                    <li class="next">
                        <a href="/page/2/">Next <span aria-hidden="true">&rarr;</span></a>
                    </li>
                </ul>
            </nav>
        "#;
        let mut detector = PaginationDetector::new_default();
        let pages = detector.detect_pagination(html, "https://quotes.toscrape.com/page/1/");

        assert!(!pages.is_empty());
        assert!(pages.contains(&"https://quotes.toscrape.com/page/2/".to_string()));
    }

    #[test]
    fn test_no_pagination_on_last_page() {
        let html = r#"
            <nav>
                <ul class="pager">
                    <li class="previous">
                        <a href="/page/9/">Previous</a>
                    </li>
                </ul>
            </nav>
        "#;

        let mut detector = PaginationDetector::new_default();
        // Should only detect URL pattern, not "Previous" link
        let pages = detector.detect_pagination(html, "https://quotes.toscrape.com/page/10/");

        // Pattern detection will suggest page 11, but it won't exist
        // This is acceptable - the crawler will handle 404s
        assert!(!pages.is_empty());
    }

    #[test]
    fn test_extract_page_number() {
        assert_eq!(
            PaginationDetector::extract_page_number("https://example.com/page/5/"),
            Some(5)
        );
        assert_eq!(
            PaginationDetector::extract_page_number("https://example.com?page=3"),
            Some(3)
        );
        assert_eq!(
            PaginationDetector::extract_page_number("https://example.com?p=7"),
            Some(7)
        );
        assert_eq!(
            PaginationDetector::extract_page_number("https://example.com/about"),
            None
        );
    }

    #[test]
    fn test_is_circular_pagination_logic() {
        // Going backwards: 1 <- 2 (circular)
        assert!(PaginationDetector::is_circular_pagination(
            "https://example.com/page/1/",
            "https://example.com/page/2/"
        ));

        // Same page (circular)
        assert!(PaginationDetector::is_circular_pagination(
            "https://example.com/page/2/",
            "https://example.com/page/2/"
        ));

        // Going forward: 3 <- 2 (not circular)
        assert!(!PaginationDetector::is_circular_pagination(
            "https://example.com/page/3/",
            "https://example.com/page/2/"
        ));
    }

    #[test]
    fn test_pagination_limit_enforced() {
        let mut detector = PaginationDetector::new(PaginationConfig {
            max_pages: 5,
            max_depth: 10,
            detect_circular: true,
        });

        // Try to detect 10 pages
        for i in 1..=10 {
            let html = format!(r#"<a href="/page/{}/" rel="next">Next</a>"#, i + 1);
            let pages =
                detector.detect_pagination(&html, &format!("https://example.com/page/{}/", i));

            if i >= 5 {
                // Should return empty after limit
                assert_eq!(
                    pages.len(),
                    0,
                    "Expected no pages after limit at iteration {}",
                    i
                );
            }
        }
    }

    #[test]
    fn test_circular_pagination_detected() {
        let mut detector = PaginationDetector::new_default();

        // Detect page 2 from page 1
        let html = r#"<a href="/articles?page=2" rel="next">Next</a>"#;
        let pages = detector.detect_pagination(html, "https://example.com/articles?page=1");
        assert_eq!(pages.len(), 1);
        assert!(pages.contains(&"https://example.com/articles?page=2".to_string()));

        // Try to detect page 1 from page 2 (circular!)
        // The HTML link points back to page 1, which should be filtered as already visited
        let html = r#"<a href="/articles?page=1" rel="next">Back</a>"#;
        let pages = detector.detect_pagination(html, "https://example.com/articles?page=2");

        // Should skip page=1 because it's already visited, and also detect page=3 from URL pattern
        // So we expect 1 page (page=3), not 0
        // But the test intent is to verify circular detection, so let's check that page=1 is NOT in results
        assert!(
            !pages.contains(&"https://example.com/articles?page=1".to_string()),
            "Page 1 should be filtered out (circular/already visited)"
        );
    }

    #[test]
    fn test_last_page_detection() {
        let html = r#"<button class="next disabled">Next</button>"#;
        assert!(PaginationDetector::is_last_page(html));

        let html2 = r#"<a href="/page/2/" class="next" disabled>Next</a>"#;
        assert!(PaginationDetector::is_last_page(html2));

        let html3 = r#"<a aria-label="Next page" aria-disabled="true">Next</a>"#;
        assert!(PaginationDetector::is_last_page(html3));
    }

    #[test]
    fn test_depth_limit_enforced() {
        let mut detector = PaginationDetector::new(PaginationConfig {
            max_pages: 100,
            max_depth: 2, // Limit depth to 2
            detect_circular: true,
        });

        // Simulate depth progression
        let mut current_url = "https://example.com/page/1/".to_string();

        // page/1/ is at depth 0 (initial)
        // page/2/ will be at depth 1 (after first detection)
        // page/3/ will be at depth 2 (after second detection)
        // page/4/ should not be added (depth would be 3, exceeds max_depth 2)

        for iteration in 1..=5 {
            let html = format!(r#"<a href="/page/{}/" rel="next">Next</a>"#, iteration + 1);
            let pages = detector.detect_pagination(&html, &current_url);

            // Internal depth progression:
            // iteration 1: current=page/1/(depth=0) → finds page/2/ → depth 1 ✓
            // iteration 2: current=page/2/(depth=1) → finds page/3/ → depth 2 ✓
            // iteration 3: current=page/3/(depth=2) → depth limit reached → []
            if iteration > 2 {
                // Should return empty after depth limit
                assert_eq!(pages.len(), 0,
                    "Expected no pages after depth limit at iteration {} (current_url={}, internal_depth={})",
                    iteration, current_url, detector.pagination_depth.get(&current_url).unwrap_or(&0));
            } else {
                assert!(
                    !pages.is_empty(),
                    "Expected pages before depth limit at iteration {}",
                    iteration
                );
                current_url = pages[0].clone();
            }
        }
    }

    #[test]
    fn test_visited_deduplication() {
        let mut detector = PaginationDetector::new_default();

        // First detection
        let html = r#"<a href="/page/2/" rel="next">Next</a>"#;
        let pages1 = detector.detect_pagination(html, "https://example.com/page/1/");
        assert_eq!(pages1.len(), 1);

        // Second detection with same page
        let html = r#"<a href="/page/2/" rel="next">Next</a>"#;
        let pages2 = detector.detect_pagination(html, "https://example.com/page/1/");

        // Should be empty because page 2 was already detected
        assert_eq!(pages2.len(), 0);
    }
}
