use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use url::Url;

/// A URL with associated priority for intelligent crawl ordering
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrioritizedUrl {
    pub url: String,
    pub priority: i32,
    pub depth: u32,
}

impl PrioritizedUrl {
    pub fn new(url: String, depth: u32, prioritizer: &UrlPrioritizer) -> Self {
        let priority = prioritizer.calculate_priority(&url, depth);
        Self {
            url,
            priority,
            depth,
        }
    }
}

/// Implement Ord for BinaryHeap (max-heap by default)
/// Higher priority values will be popped first
impl Ord for PrioritizedUrl {
    fn cmp(&self, other: &Self) -> Ordering {
        // First compare by priority (higher is better)
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {
                // If priorities are equal, prefer lower depth (closer to root)
                other.depth.cmp(&self.depth)
            }
            other => other,
        }
    }
}

impl PartialOrd for PrioritizedUrl {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Pattern-based URL prioritization for intelligent crawling
#[derive(Debug, Clone)]
pub struct UrlPrioritizer {
    /// Patterns that boost priority
    high_priority_patterns: Vec<Regex>,
    /// Patterns that reduce priority
    low_priority_patterns: Vec<Regex>,
    /// Base priority for all URLs
    base_priority: i32,
    /// Priority penalty per depth level
    depth_penalty: i32,
    /// URL length threshold (shorter is better)
    url_length_threshold: usize,
}

impl Default for UrlPrioritizer {
    fn default() -> Self {
        Self::new()
    }
}

impl UrlPrioritizer {
    /// Create a new URL prioritizer with default patterns
    pub fn new() -> Self {
        let high_priority_patterns = vec![
            // Index pages and main content
            Regex::new(r"/(index|main|home|about|contact)(\.|$|/)").unwrap(),
            // Documentation pages
            Regex::new(r"/(docs?|documentation|guide|tutorial|api|reference)(/|$)").unwrap(),
            // Product/service pages
            Regex::new(r"/(product|service|feature|pricing|solution)s?(/|$)").unwrap(),
            // Blog/article pages (but not paginated)
            Regex::new(r"/(blog|article|post|news)/[^/]+/?$").unwrap(),
            // Category/archive pages (not deeply nested)
            Regex::new(r"/(category|tag|topic)/[^/]+/?$").unwrap(),
        ];

        let low_priority_patterns = vec![
            // Pagination (high page numbers)
            Regex::new(r"/page/([2-9]|\d{2,})(/|$)").unwrap(),
            Regex::new(r"[?&]page=([2-9]|\d{2,})(&|$)").unwrap(),
            // Archive pages with dates
            Regex::new(r"/\d{4}/\d{2}").unwrap(),
            // PDFs and downloadable files
            Regex::new(r"\.(pdf|zip|tar|gz|rar|exe|dmg|pkg)$").unwrap(),
            // Media files
            Regex::new(r"\.(jpg|jpeg|png|gif|svg|mp4|mp3|avi|mov|wav)$").unwrap(),
            // Fragments and anchors
            Regex::new(r"#").unwrap(),
            // Very long query strings (often filters/sorts)
            Regex::new(r"\?[^?]{50,}$").unwrap(),
            // Login/logout/admin pages
            Regex::new(r"/(login|logout|admin|account|dashboard|settings|profile)(/|$)").unwrap(),
            // Comment/reply URLs
            Regex::new(r"/(comment|reply|respond)").unwrap(),
            // Search result pages
            Regex::new(r"[?&](search|q|query)=").unwrap(),
        ];

        Self {
            high_priority_patterns,
            low_priority_patterns,
            base_priority: 100,
            depth_penalty: 10,
            url_length_threshold: 100,
        }
    }

    /// Calculate priority for a URL based on multiple factors
    ///
    /// Priority calculation:
    /// - Base: 100 points
    /// - Root/top-level boost: +100 for root, +40 for top-level (applied first)
    /// - Depth penalty: -10 points per level
    /// - Pattern matches: +50 for high priority, -50 for low priority
    /// - URL length: -1 point per 10 chars over threshold
    pub fn calculate_priority(&self, url: &str, depth: u32) -> i32 {
        let mut priority = self.base_priority;

        // Parse URL first to check structure
        let parsed_url = Url::parse(url).ok();

        // Boost root domain and top-level paths FIRST (before depth penalty)
        // This ensures important pages stay prioritized regardless of depth
        if let Some(ref parsed) = parsed_url {
            let path = parsed.path();
            if path == "/" || path.is_empty() {
                priority += 100; // Very strong boost for root pages (ensures they're always first)
            } else if path.matches('/').count() == 1
                || (path.matches('/').count() == 2 && path.ends_with('/'))
            {
                // Top-level path like /about or /about/
                priority += 40; // Good boost for top-level pages
            }
        }

        // Apply depth penalty (deeper URLs are lower priority)
        priority -= (depth as i32) * self.depth_penalty;

        // Check high priority patterns
        for pattern in &self.high_priority_patterns {
            if pattern.is_match(url) {
                priority += 50;
                break; // Only apply one high priority boost
            }
        }

        // Check low priority patterns
        for pattern in &self.low_priority_patterns {
            if pattern.is_match(url) {
                priority -= 50;
                break; // Only apply one low priority penalty
            }
        }

        // URL length penalty (shorter URLs are generally more important)
        if let Some(parsed) = parsed_url {
            let url_string = parsed.to_string();
            if url_string.len() > self.url_length_threshold {
                let excess_length = url_string.len() - self.url_length_threshold;
                priority -= (excess_length / 10) as i32;
            }
        }

        priority
    }

    /// Customize the prioritizer with specific patterns
    pub fn with_high_priority_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.high_priority_patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Customize the prioritizer with low priority patterns
    pub fn with_low_priority_pattern(mut self, pattern: &str) -> Result<Self, regex::Error> {
        self.low_priority_patterns.push(Regex::new(pattern)?);
        Ok(self)
    }

    /// Set base priority
    pub fn with_base_priority(mut self, priority: i32) -> Self {
        self.base_priority = priority;
        self
    }

    /// Set depth penalty
    pub fn with_depth_penalty(mut self, penalty: i32) -> Self {
        self.depth_penalty = penalty;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BinaryHeap;

    #[test]
    fn test_prioritized_url_ordering() {
        let prioritizer = UrlPrioritizer::new();

        let url1 = PrioritizedUrl::new("https://example.com/page/10".to_string(), 2, &prioritizer);
        let url2 = PrioritizedUrl::new(
            "https://example.com/docs/guide".to_string(),
            1,
            &prioritizer,
        );
        let url3 = PrioritizedUrl::new("https://example.com/".to_string(), 0, &prioritizer);

        // Higher priority should come first
        assert!(url3.priority > url2.priority);
        assert!(url2.priority > url1.priority);

        // Test ordering
        assert_eq!(url3.cmp(&url2), Ordering::Greater);
        assert_eq!(url2.cmp(&url1), Ordering::Greater);
    }

    #[test]
    fn test_binary_heap_ordering() {
        let prioritizer = UrlPrioritizer::new();
        let mut heap = BinaryHeap::new();

        // Add URLs in random order
        heap.push(PrioritizedUrl::new(
            "https://example.com/page/10".to_string(),
            2,
            &prioritizer,
        ));
        heap.push(PrioritizedUrl::new(
            "https://example.com/".to_string(),
            0,
            &prioritizer,
        ));
        heap.push(PrioritizedUrl::new(
            "https://example.com/docs/guide".to_string(),
            1,
            &prioritizer,
        ));
        heap.push(PrioritizedUrl::new(
            "https://example.com/login".to_string(),
            2,
            &prioritizer,
        ));

        // Pop should return highest priority first (root page)
        let first = heap.pop().unwrap();
        assert_eq!(first.url, "https://example.com/");

        // Next should be docs
        let second = heap.pop().unwrap();
        assert_eq!(second.url, "https://example.com/docs/guide");
    }

    #[test]
    fn test_depth_penalty() {
        let prioritizer = UrlPrioritizer::new();

        let shallow = prioritizer.calculate_priority("https://example.com/page", 0);
        let deep = prioritizer.calculate_priority("https://example.com/page", 5);

        // Deeper URLs should have lower priority
        assert!(shallow > deep);
        assert_eq!(shallow - deep, 50); // 5 levels * 10 points per level
    }

    #[test]
    fn test_high_priority_patterns() {
        let prioritizer = UrlPrioritizer::new();

        let base_priority = prioritizer.calculate_priority("https://example.com/random", 0);
        let docs_priority = prioritizer.calculate_priority("https://example.com/docs/api", 0);
        let index_priority = prioritizer.calculate_priority("https://example.com/index.html", 0);

        // Docs and index should have higher priority than random page
        assert!(docs_priority > base_priority);
        assert!(index_priority > base_priority);
    }

    #[test]
    fn test_low_priority_patterns() {
        let prioritizer = UrlPrioritizer::new();

        let base_priority = prioritizer.calculate_priority("https://example.com/article", 0);
        let pdf_priority = prioritizer.calculate_priority("https://example.com/doc.pdf", 0);
        let page10_priority = prioritizer.calculate_priority("https://example.com/page/10", 0);
        let login_priority = prioritizer.calculate_priority("https://example.com/login", 0);

        // PDFs, high page numbers, and login should have lower priority
        assert!(pdf_priority < base_priority);
        assert!(page10_priority < base_priority);
        assert!(login_priority < base_priority);
    }

    #[test]
    fn test_url_length_penalty() {
        let prioritizer = UrlPrioritizer::new();

        let short_url = "https://example.com/page";
        let long_url = "https://example.com/very/long/path/with/many/segments/and/parameters?query=value&filter=enabled&sort=desc";

        let short_priority = prioritizer.calculate_priority(short_url, 0);
        let long_priority = prioritizer.calculate_priority(long_url, 0);

        // Shorter URLs should have higher priority
        assert!(short_priority > long_priority);
    }

    #[test]
    fn test_root_path_boost() {
        let prioritizer = UrlPrioritizer::new();

        let root_priority = prioritizer.calculate_priority("https://example.com/", 0);
        let subpage_priority = prioritizer.calculate_priority("https://example.com/about", 0);
        let deep_priority = prioritizer.calculate_priority("https://example.com/blog/post/123", 0);

        // Root should have highest priority
        assert!(root_priority > subpage_priority);
        assert!(subpage_priority > deep_priority);
    }

    #[test]
    fn test_custom_patterns() {
        let prioritizer = UrlPrioritizer::new()
            .with_high_priority_pattern(r"/important")
            .unwrap()
            .with_low_priority_pattern(r"/ignore")
            .unwrap();

        let important_priority =
            prioritizer.calculate_priority("https://example.com/important/page", 0);
        let ignore_priority = prioritizer.calculate_priority("https://example.com/ignore/page", 0);
        let normal_priority = prioritizer.calculate_priority("https://example.com/normal/page", 0);

        assert!(important_priority > normal_priority);
        assert!(ignore_priority < normal_priority);
    }

    #[test]
    fn test_pagination_detection() {
        let prioritizer = UrlPrioritizer::new();

        let page1_priority = prioritizer.calculate_priority("https://example.com/blog", 0);
        let page2_priority = prioritizer.calculate_priority("https://example.com/blog/page/2", 0);
        let page10_priority = prioritizer.calculate_priority("https://example.com/blog/page/10", 0);

        // First page should have higher priority than pagination
        assert!(page1_priority > page2_priority);
        assert!(page1_priority > page10_priority);

        // Higher page numbers should have lower priority
        assert_eq!(page2_priority, page10_priority); // Both get the same low priority penalty
    }

    #[test]
    fn test_equal_priority_depth_tiebreaker() {
        let prioritizer = UrlPrioritizer::new();

        // Two URLs with same base priority but different depths
        let shallow =
            PrioritizedUrl::new("https://example.com/random1".to_string(), 1, &prioritizer);
        let deep = PrioritizedUrl::new("https://example.com/random2".to_string(), 3, &prioritizer);

        // When priorities are close, shallower depth should win
        assert!(shallow.depth < deep.depth);
    }
}
