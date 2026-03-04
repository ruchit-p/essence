use glob_match::glob_match;
use url::Url;

/// Check if a URL matches any of the given glob patterns
pub fn matches_pattern(url: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }

    // Parse the URL to get the path
    let parsed_url = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let path = parsed_url.path();

    // Check if any pattern matches
    for pattern in patterns {
        if glob_match(pattern, path) {
            return true;
        }

        // Also try matching against the full URL
        if glob_match(pattern, url) {
            return true;
        }
    }

    false
}

/// Check if a URL should be crawled based on include/exclude patterns
pub fn should_crawl_url(
    url: &str,
    include_paths: &Option<Vec<String>>,
    exclude_paths: &Option<Vec<String>>,
) -> bool {
    // If exclude patterns exist and URL matches any, skip it
    if let Some(excludes) = exclude_paths {
        if matches_pattern(url, excludes) {
            return false;
        }
    }

    // If include patterns exist, URL must match at least one
    if let Some(includes) = include_paths {
        if !includes.is_empty() {
            return matches_pattern(url, includes);
        }
    }

    // Default: crawl the URL
    true
}

/// Check if a URL is in the same domain as the base URL
pub fn is_same_domain(url: &str, base_url: &str) -> bool {
    let url_parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let base_parsed = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    url_parsed.host_str() == base_parsed.host_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern_simple() {
        let patterns = vec!["/blog/*".to_string()];
        assert!(matches_pattern(
            "https://example.com/blog/post-1",
            &patterns
        ));
        assert!(!matches_pattern("https://example.com/about", &patterns));
    }

    #[test]
    fn test_matches_pattern_multiple() {
        let patterns = vec!["/blog/*".to_string(), "/news/*".to_string()];
        assert!(matches_pattern(
            "https://example.com/blog/post-1",
            &patterns
        ));
        assert!(matches_pattern(
            "https://example.com/news/article-1",
            &patterns
        ));
        assert!(!matches_pattern("https://example.com/about", &patterns));
    }

    #[test]
    fn test_should_crawl_url_with_excludes() {
        let excludes = Some(vec!["/admin/*".to_string(), "/private/*".to_string()]);
        let includes = None;

        assert!(!should_crawl_url(
            "https://example.com/admin/dashboard",
            &includes,
            &excludes
        ));
        assert!(!should_crawl_url(
            "https://example.com/private/data",
            &includes,
            &excludes
        ));
        assert!(should_crawl_url(
            "https://example.com/public/page",
            &includes,
            &excludes
        ));
    }

    #[test]
    fn test_should_crawl_url_with_includes() {
        let includes = Some(vec!["/blog/*".to_string()]);
        let excludes = None;

        assert!(should_crawl_url(
            "https://example.com/blog/post-1",
            &includes,
            &excludes
        ));
        assert!(!should_crawl_url(
            "https://example.com/about",
            &includes,
            &excludes
        ));
    }

    #[test]
    fn test_should_crawl_url_with_both() {
        let includes = Some(vec!["/blog/*".to_string()]);
        let excludes = Some(vec!["/blog/draft/*".to_string()]);

        assert!(should_crawl_url(
            "https://example.com/blog/post-1",
            &includes,
            &excludes
        ));
        assert!(!should_crawl_url(
            "https://example.com/blog/draft/post-2",
            &includes,
            &excludes
        ));
        assert!(!should_crawl_url(
            "https://example.com/about",
            &includes,
            &excludes
        ));
    }

    #[test]
    fn test_is_same_domain() {
        assert!(is_same_domain(
            "https://example.com/page1",
            "https://example.com/page2"
        ));
        assert!(!is_same_domain(
            "https://example.com/page1",
            "https://other.com/page2"
        ));
        assert!(!is_same_domain(
            "https://sub.example.com/page1",
            "https://example.com/page2"
        ));
    }
}
