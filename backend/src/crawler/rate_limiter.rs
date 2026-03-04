use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use url::Url;

/// Per-domain rate limiter to ensure respectful crawling
pub struct DomainRateLimiter {
    limiters: Arc<Mutex<HashMap<String, Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>>,
    default_quota: Quota,
}

impl DomainRateLimiter {
    /// Create a new rate limiter with a default requests-per-second limit
    pub fn new(requests_per_second: u32) -> Self {
        let quota = Quota::per_second(
            NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::new(2).unwrap()),
        );

        Self {
            limiters: Arc::new(Mutex::new(HashMap::new())),
            default_quota: quota,
        }
    }

    /// Wait until we're allowed to make a request to this domain
    pub async fn wait_for_permission(&self, url: &str) -> Result<(), String> {
        let domain = Self::extract_domain(url)?;

        let limiter = {
            let mut limiters = self
                .limiters
                .lock()
                .map_err(|e| format!("Failed to acquire lock: {}", e))?;
            
            limiters
                .entry(domain.clone())
                .or_insert_with(|| Arc::new(RateLimiter::direct(self.default_quota)))
                .clone()
        };

        // Wait until we have permission (non-blocking in async context)
        limiter.until_ready().await;

        tracing::debug!(
            "Rate limiter: Granted permission for domain: {}",
            domain
        );

        Ok(())
    }

    /// Extract domain from URL
    fn extract_domain(url: &str) -> Result<String, String> {
        let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

        parsed
            .host_str()
            .map(|h| h.to_string())
            .ok_or_else(|| "No host in URL".to_string())
    }

    /// Set custom rate for specific domain
    pub fn set_domain_rate(&self, domain: &str, requests_per_second: u32) {
        let quota = Quota::per_second(
            NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::new(1).unwrap()),
        );
        let limiter = Arc::new(RateLimiter::direct(quota));

        if let Ok(mut limiters) = self.limiters.lock() {
            limiters.insert(domain.to_string(), limiter);
            tracing::info!(
                "Set custom rate limit for {}: {} req/sec",
                domain,
                requests_per_second
            );
        }
    }
}

impl Default for DomainRateLimiter {
    fn default() -> Self {
        Self::new(2) // 2 requests/second default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_rate_limiter_enforces_delay() {
        let limiter = DomainRateLimiter::new(2); // 2 req/sec

        let start = Instant::now();

        // Make 3 requests to same domain
        limiter
            .wait_for_permission("https://example.com/1")
            .await
            .unwrap();
        limiter
            .wait_for_permission("https://example.com/2")
            .await
            .unwrap();
        limiter
            .wait_for_permission("https://example.com/3")
            .await
            .unwrap();

        let elapsed = start.elapsed();

        // Should take at least 500ms (3 requests at 2/sec = 2 allowed immediately, 1 delayed)
        assert!(
            elapsed.as_millis() >= 400,
            "Expected at least 400ms delay, got {}ms",
            elapsed.as_millis()
        );
    }

    #[tokio::test]
    async fn test_different_domains_not_limited() {
        let limiter = DomainRateLimiter::new(1); // 1 req/sec

        let start = Instant::now();

        // Different domains shouldn't affect each other
        limiter
            .wait_for_permission("https://example.com")
            .await
            .unwrap();
        limiter
            .wait_for_permission("https://other.com")
            .await
            .unwrap();

        let elapsed = start.elapsed();

        // Should be instant (different domains) - allow some overhead for timing variability
        assert!(
            elapsed.as_millis() < 300,
            "Different domains should not block each other, got {}ms",
            elapsed.as_millis()
        );
    }

    #[tokio::test]
    async fn test_custom_domain_rate() {
        let limiter = DomainRateLimiter::new(10); // 10 req/sec default

        // Set custom rate for specific domain
        limiter.set_domain_rate("slow.example.com", 1); // 1 req/sec

        let start = Instant::now();

        // Make 2 requests to slow domain
        limiter
            .wait_for_permission("https://slow.example.com/1")
            .await
            .unwrap();
        limiter
            .wait_for_permission("https://slow.example.com/2")
            .await
            .unwrap();

        let elapsed = start.elapsed();

        // Should be delayed by custom rate (1 req/sec)
        assert!(
            elapsed.as_millis() >= 800,
            "Custom rate should be enforced, got {}ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            DomainRateLimiter::extract_domain("https://example.com/path").unwrap(),
            "example.com"
        );
        assert_eq!(
            DomainRateLimiter::extract_domain("https://sub.example.com").unwrap(),
            "sub.example.com"
        );
        assert!(DomainRateLimiter::extract_domain("invalid").is_err());
    }
}
