//! Enhanced robots.txt parser with caching and crawl-delay support
//!
//! Features:
//! - In-memory caching with TTL (24 hours default)
//! - Crawl-Delay directive parsing
//! - eTLD+1 domain extraction (future: use psl crate)
//! - Configurable user agent
//! - Graceful fallback if robots.txt unavailable

use crate::error::{Result, ScrapeError};
use moka::future::Cache;
use std::sync::LazyLock;
use reqwest::Client;
use robotstxt::DefaultMatcher;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};
use url::Url;

/// Cached robots.txt data
#[derive(Debug, Clone)]
pub struct RobotsData {
    /// Raw robots.txt content
    pub content: String,
    /// Crawl-Delay value (in seconds) if specified
    pub crawl_delay: Option<f64>,
    /// Whether the URL is allowed
    pub allowed: bool,
}

/// Global robots.txt cache
/// Key: domain (e.g., "example.com")
/// Value: RobotsData
static ROBOTS_CACHE: LazyLock<Arc<Cache<String, RobotsData>>> = LazyLock::new(|| {
    Arc::new(
        Cache::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(24 * 3600)) // 24 hours
            .build(),
    )
});

/// Extract domain from URL (uses proper eTLD+1 extraction)
fn extract_domain(url: &Url) -> Result<String> {
    crate::utils::etld::extract_etld_plus_one(url.as_str())
}

/// Parse Crawl-Delay directive from robots.txt
/// 
/// Looks for lines like:
/// - `Crawl-delay: 1`
/// - `Crawl-Delay: 0.5`
/// 
/// Returns the delay in seconds, or None if not specified
fn parse_crawl_delay(robots_txt: &str, user_agent: &str) -> Option<f64> {
    let mut in_user_agent_block = false;
    let mut crawl_delay: Option<f64> = None;

    for line in robots_txt.lines() {
        let line = line.trim();

        // Check if this is the user agent we're looking for
        if line.to_lowercase().starts_with("user-agent:") {
            let agent = line[11..].trim();
            in_user_agent_block = agent == "*" || agent.eq_ignore_ascii_case(user_agent);
        }

        // If we're in the right user-agent block, look for Crawl-delay
        if in_user_agent_block && line.to_lowercase().starts_with("crawl-delay:") {
            let delay_str = line[12..].trim();
            if let Ok(delay) = delay_str.parse::<f64>() {
                crawl_delay = Some(delay);
                debug!("Parsed Crawl-Delay: {} seconds", delay);
            }
        }
    }

    crawl_delay
}

/// Fetch and parse robots.txt for a domain
async fn fetch_robots_txt(domain: &str, user_agent: &str) -> Result<RobotsData> {
    let robots_url = format!("https://{}/robots.txt", domain);
    debug!("Fetching robots.txt from: {}", robots_url);

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| ScrapeError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let response = match client.get(&robots_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            warn!("Failed to fetch robots.txt from {}: {}", robots_url, e);
            // If robots.txt doesn't exist, allow by default
            return Ok(RobotsData {
                content: String::new(),
                crawl_delay: None,
                allowed: true,
            });
        }
    };

    if !response.status().is_success() {
        warn!("robots.txt not found at {} (status: {})", robots_url, response.status());
        // If robots.txt doesn't exist, allow by default
        return Ok(RobotsData {
            content: String::new(),
            crawl_delay: None,
            allowed: true,
        });
    }

    let robots_txt = response
        .text()
        .await
        .map_err(|e| ScrapeError::Internal(format!("Failed to read robots.txt: {}", e)))?;

    // Parse crawl-delay directive
    let crawl_delay = parse_crawl_delay(&robots_txt, user_agent);

    Ok(RobotsData {
        content: robots_txt,
        crawl_delay,
        allowed: true, // Will be determined per-URL
    })
}

/// Check if a URL is allowed by robots.txt (with caching)
/// 
/// This function:
/// 1. Extracts the domain from the URL
/// 2. Checks if robots.txt is cached
/// 3. If not cached, fetches and caches it
/// 4. Checks if the specific path is allowed
/// 5. Returns the crawl delay if specified
pub async fn is_allowed_cached(url: &str, user_agent: &str) -> Result<(bool, Option<f64>)> {
    let parsed_url = Url::parse(url)
        .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid URL: {}", e)))?;

    let domain = extract_domain(&parsed_url)?;

    // Check cache first
    let robots_data = if let Some(cached) = ROBOTS_CACHE.get(&domain).await {
        debug!("Robots.txt cache hit for domain: {}", domain);
        cached
    } else {
        debug!("Robots.txt cache miss for domain: {}", domain);
        let data = fetch_robots_txt(&domain, user_agent).await?;
        ROBOTS_CACHE.insert(domain.clone(), data.clone()).await;
        data
    };

    // Check if the specific path is allowed
    let path = parsed_url.path();
    let allowed = if robots_data.content.is_empty() {
        // No robots.txt, allow by default
        true
    } else {
        let mut matcher = DefaultMatcher::default();
        matcher.one_agent_allowed_by_robots(&robots_data.content, user_agent, path)
    };

    Ok((allowed, robots_data.crawl_delay))
}

/// Check robots.txt with default user agent "Essence"
pub async fn is_allowed_default_cached(url: &str) -> Result<(bool, Option<f64>)> {
    is_allowed_cached(url, "Essence").await
}

/// Clear the robots.txt cache (useful for testing)
#[cfg(test)]
pub async fn clear_cache() {
    ROBOTS_CACHE.invalidate_all();
    ROBOTS_CACHE.run_pending_tasks().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_extraction() {
        let url = Url::parse("https://www.example.com/path/to/page").unwrap();
        let domain = extract_domain(&url).unwrap();
        // extract_domain uses eTLD+1, so www.example.com → example.com
        assert_eq!(domain, "example.com");
    }

    #[test]
    fn test_crawl_delay_parsing() {
        let robots_txt = r#"
User-agent: *
Crawl-delay: 2

User-agent: Essence
Crawl-delay: 0.5
Disallow: /admin
        "#;

        let delay_all = parse_crawl_delay(robots_txt, "*");
        assert_eq!(delay_all, Some(2.0));

        let delay_essence = parse_crawl_delay(robots_txt, "Essence");
        assert_eq!(delay_essence, Some(0.5));

        // OtherBot matches the wildcard (*) block, so gets its crawl delay
        let delay_other = parse_crawl_delay(robots_txt, "OtherBot");
        assert_eq!(delay_other, Some(2.0));
    }

    #[test]
    fn test_crawl_delay_parsing_case_insensitive() {
        let robots_txt = r#"
user-agent: *
crawl-delay: 1.5
        "#;

        let delay = parse_crawl_delay(robots_txt, "*");
        assert_eq!(delay, Some(1.5));
    }

    #[tokio::test]
    async fn test_cache() {
        clear_cache().await;

        // First call - should cache
        let domain = "example.com";
        let robots_data = RobotsData {
            content: "User-agent: *\nDisallow: /admin".to_string(),
            crawl_delay: Some(1.0),
            allowed: true,
        };

        ROBOTS_CACHE.insert(domain.to_string(), robots_data.clone()).await;

        // Second call - should hit cache
        let cached = ROBOTS_CACHE.get(domain).await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().crawl_delay, Some(1.0));
    }
}
