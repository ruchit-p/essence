use governor::{
    clock::DefaultClock, state::direct::NotKeyed, state::InMemoryState, Quota, RateLimiter,
};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Rate limit error
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded for API key")]
    Exceeded,
}

/// Type alias for the rate limiter we use
type ApiRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Per-API-key rate limiter
pub struct ApiKeyRateLimiter {
    /// Map of API key to rate limiter
    limiters: Arc<RwLock<HashMap<String, Arc<ApiRateLimiter>>>>,

    /// Default rate limit (requests per minute)
    default_limit: NonZeroU32,

    /// Per-user/API key limits
    api_key_limits: HashMap<String, NonZeroU32>,
}

impl ApiKeyRateLimiter {
    /// Create a new API key rate limiter
    pub fn new(default_per_minute: u32, api_key_limits: HashMap<String, u32>) -> Self {
        let default_limit = NonZeroU32::new(default_per_minute)
            .unwrap_or_else(|| NonZeroU32::new(60).unwrap());

        let mut limits_map = HashMap::new();
        for (key, limit) in api_key_limits {
            if let Some(non_zero) = NonZeroU32::new(limit) {
                limits_map.insert(key, non_zero);
            }
        }

        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
            default_limit,
            api_key_limits: limits_map,
        }
    }

    /// Check rate limit for a specific API key
    pub async fn check_limit(&self, api_key: &str) -> Result<(), RateLimitError> {
        let limiter = self.get_or_create_limiter(api_key).await;
        
        match limiter.check() {
            Ok(_) => {
                debug!("Rate limit check passed for API key: {}", Self::redact_key(api_key));
                Ok(())
            }
            Err(_) => {
                debug!("Rate limit exceeded for API key: {}", Self::redact_key(api_key));
                Err(RateLimitError::Exceeded)
            }
        }
    }

    /// Get or create a rate limiter for the given API key
    async fn get_or_create_limiter(&self, api_key: &str) -> Arc<ApiRateLimiter> {
        // Check if limiter already exists
        {
            let limiters = self.limiters.read().await;
            if let Some(limiter) = limiters.get(api_key) {
                return Arc::clone(limiter);
            }
        }

        // Create new limiter
        let limit = self.get_limit_for_key(api_key);
        let quota = Quota::per_minute(limit);
        let limiter = Arc::new(RateLimiter::direct(quota));

        // Store it
        {
            let mut limiters = self.limiters.write().await;
            limiters.insert(api_key.to_string(), Arc::clone(&limiter));
        }

        debug!(
            "Created rate limiter for API key {} with limit: {}/minute",
            Self::redact_key(api_key),
            limit
        );

        limiter
    }

    /// Get the rate limit for a specific API key
    fn get_limit_for_key(&self, api_key: &str) -> NonZeroU32 {
        // Try to find user ID from API key mapping
        // For now, we use the API key directly as the user identifier
        self.api_key_limits
            .get(api_key)
            .copied()
            .unwrap_or(self.default_limit)
    }

    /// Redact API key for logging (show only first 8 chars)
    fn redact_key(key: &str) -> String {
        if key.len() > 8 {
            format!("{}...", &key[..8])
        } else {
            "***".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_default_limit() {
        let limiter = ApiKeyRateLimiter::new(2, HashMap::new());

        // First two requests should pass
        assert!(limiter.check_limit("test_key").await.is_ok());
        assert!(limiter.check_limit("test_key").await.is_ok());

        // Third request should fail (limit is 2 per minute)
        assert!(limiter.check_limit("test_key").await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_per_user_limit() {
        let mut limits = HashMap::new();
        limits.insert("user1".to_string(), 5);
        limits.insert("user2".to_string(), 2);

        let limiter = ApiKeyRateLimiter::new(10, limits);

        // user1 should be able to make 5 requests
        for _ in 0..5 {
            assert!(limiter.check_limit("user1").await.is_ok());
        }
        assert!(limiter.check_limit("user1").await.is_err());

        // user2 should be able to make 2 requests
        for _ in 0..2 {
            assert!(limiter.check_limit("user2").await.is_ok());
        }
        assert!(limiter.check_limit("user2").await.is_err());

        // user3 (not in limits map) should get default limit of 10
        for _ in 0..10 {
            assert!(limiter.check_limit("user3").await.is_ok());
        }
        assert!(limiter.check_limit("user3").await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_separate_keys() {
        let limiter = ApiKeyRateLimiter::new(2, HashMap::new());

        // Each key should have its own limit
        assert!(limiter.check_limit("key1").await.is_ok());
        assert!(limiter.check_limit("key2").await.is_ok());
        assert!(limiter.check_limit("key1").await.is_ok());
        assert!(limiter.check_limit("key2").await.is_ok());

        // Both should be at limit now
        assert!(limiter.check_limit("key1").await.is_err());
        assert!(limiter.check_limit("key2").await.is_err());
    }

    #[test]
    fn test_redact_key() {
        assert_eq!(ApiKeyRateLimiter::redact_key("short"), "***");
        assert_eq!(
            ApiKeyRateLimiter::redact_key("sk-ant-api03-longkeyhere"),
            "sk-ant-a..."
        );
    }
}
