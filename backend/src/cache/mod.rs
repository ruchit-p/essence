//! Smart caching layer with content, robots.txt, and redirect caching
//!
//! This module provides an intelligent caching system using the moka crate
//! with appropriate TTLs and cache-first strategies.

use blake3::Hasher;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Content cache TTL (default: 1 hour)
    pub content_ttl_secs: u64,
    /// Robots.txt cache TTL (default: 24 hours)
    pub robots_ttl_secs: u64,
    /// Redirect cache TTL (default: 1 hour)
    pub redirect_ttl_secs: u64,
    /// Max cache entries
    pub max_capacity: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            content_ttl_secs: 3600,  // 1 hour
            robots_ttl_secs: 86400,  // 24 hours
            redirect_ttl_secs: 3600, // 1 hour
            max_capacity: 10000,     // 10k entries
        }
    }
}

/// Cached content with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedContent {
    pub html: String,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub headers: Vec<(String, String)>,
    pub cached_at: u64,
}

/// Cached robots.txt content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRobots {
    pub content: String,
    pub cached_at: u64,
}

/// Cached redirect mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRedirect {
    pub target_url: String,
    pub status_code: u16,
    pub cached_at: u64,
}

/// Cache metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    pub content_hits: u64,
    pub content_misses: u64,
    pub robots_hits: u64,
    pub robots_misses: u64,
    pub redirect_hits: u64,
    pub redirect_misses: u64,
}

/// Main cache layer with three specialized caches
pub struct CacheLayer {
    /// Content cache: maps URL+headers -> cached HTML/content
    content_cache: Cache<String, CachedContent>,
    /// Robots.txt cache: maps domain -> robots.txt content
    robots_cache: Cache<String, CachedRobots>,
    /// Redirect cache: maps URL -> target URL
    redirect_cache: Cache<String, CachedRedirect>,
    /// Cache metrics
    metrics: Arc<tokio::sync::RwLock<CacheMetrics>>,
    /// Configuration
    #[allow(dead_code)]
    config: CacheConfig,
}

impl CacheLayer {
    /// Create a new cache layer with default configuration
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create a new cache layer with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        info!(
            "Initializing cache layer: content_ttl={}s, robots_ttl={}s, redirect_ttl={}s, max_capacity={}",
            config.content_ttl_secs, config.robots_ttl_secs, config.redirect_ttl_secs, config.max_capacity
        );

        let content_cache = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(Duration::from_secs(config.content_ttl_secs))
            .build();

        let robots_cache = Cache::builder()
            .max_capacity(config.max_capacity / 10) // Fewer robots.txt entries
            .time_to_live(Duration::from_secs(config.robots_ttl_secs))
            .build();

        let redirect_cache = Cache::builder()
            .max_capacity(config.max_capacity / 10) // Fewer redirect entries
            .time_to_live(Duration::from_secs(config.redirect_ttl_secs))
            .build();

        Self {
            content_cache,
            robots_cache,
            redirect_cache,
            metrics: Arc::new(tokio::sync::RwLock::new(CacheMetrics::default())),
            config,
        }
    }

    /// Generate a cache key from URL and optional headers
    ///
    /// Uses BLAKE3 hashing for fast, collision-resistant cache keys.
    ///
    /// # Example
    /// ```ignore
    /// let key = CacheLayer::generate_cache_key("https://example.com", None);
    /// ```
    pub fn generate_cache_key(url: &str, headers: Option<&[(String, String)]>) -> String {
        let mut hasher = Hasher::new();
        hasher.update(url.as_bytes());

        if let Some(headers) = headers {
            for (key, value) in headers {
                hasher.update(key.as_bytes());
                hasher.update(value.as_bytes());
            }
        }

        hasher.finalize().to_hex().to_string()
    }

    /// Get cached content or fetch it using the provided async function
    ///
    /// This implements a cache-first strategy where the cache is checked first,
    /// and only if there's a miss does it call the fetch function.
    ///
    /// # Example
    /// ```ignore
    /// let content = cache.get_or_fetch_content(
    ///     "https://example.com",
    ///     None,
    ///     || async {
    ///         // Fetch logic here
    ///         Ok(CachedContent { ... })
    ///     }
    /// ).await?;
    /// ```
    pub async fn get_or_fetch_content<F, Fut>(
        &self,
        url: &str,
        headers: Option<&[(String, String)]>,
        fetch_fn: F,
    ) -> Result<CachedContent, crate::error::ScrapeError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<CachedContent, crate::error::ScrapeError>>,
    {
        let cache_key = Self::generate_cache_key(url, headers);

        // Try to get from cache
        if let Some(cached) = self.content_cache.get(&cache_key).await {
            debug!("Cache hit for URL: {}", url);
            let mut metrics_data = self.metrics.write().await;
            metrics_data.content_hits += 1;
            drop(metrics_data);

            return Ok(cached);
        }

        // Cache miss - fetch content
        debug!("Cache miss for URL: {}", url);
        let mut metrics_data = self.metrics.write().await;
        metrics_data.content_misses += 1;
        drop(metrics_data);

        let content = fetch_fn().await?;

        // Store in cache
        self.content_cache.insert(cache_key, content.clone()).await;

        Ok(content)
    }

    /// Get cached robots.txt or fetch it
    pub async fn get_or_fetch_robots<F, Fut>(
        &self,
        domain: &str,
        fetch_fn: F,
    ) -> Result<CachedRobots, crate::error::ScrapeError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<CachedRobots, crate::error::ScrapeError>>,
    {
        // Try to get from cache
        if let Some(cached) = self.robots_cache.get(domain).await {
            debug!("Cache hit for robots.txt: {}", domain);
            let mut metrics_data = self.metrics.write().await;
            metrics_data.robots_hits += 1;
            drop(metrics_data);

            return Ok(cached);
        }

        // Cache miss - fetch robots.txt
        debug!("Cache miss for robots.txt: {}", domain);
        let mut metrics_data = self.metrics.write().await;
        metrics_data.robots_misses += 1;
        drop(metrics_data);

        let robots = fetch_fn().await?;

        // Store in cache
        self.robots_cache
            .insert(domain.to_string(), robots.clone())
            .await;

        Ok(robots)
    }

    /// Get cached redirect or fetch it
    pub async fn get_or_fetch_redirect<F, Fut>(
        &self,
        url: &str,
        fetch_fn: F,
    ) -> Result<Option<CachedRedirect>, crate::error::ScrapeError>
    where
        F: FnOnce() -> Fut,
        Fut:
            std::future::Future<Output = Result<Option<CachedRedirect>, crate::error::ScrapeError>>,
    {
        // Try to get from cache
        if let Some(cached) = self.redirect_cache.get(url).await {
            debug!("Cache hit for redirect: {}", url);
            let mut metrics_data = self.metrics.write().await;
            metrics_data.redirect_hits += 1;
            drop(metrics_data);

            return Ok(Some(cached));
        }

        // Cache miss - fetch redirect
        debug!("Cache miss for redirect: {}", url);
        let mut metrics_data = self.metrics.write().await;
        metrics_data.redirect_misses += 1;
        drop(metrics_data);

        let redirect = fetch_fn().await?;

        // Store in cache if redirect exists
        if let Some(ref redir) = redirect {
            self.redirect_cache
                .insert(url.to_string(), redir.clone())
                .await;
        }

        Ok(redirect)
    }

    /// Get current cache metrics
    pub async fn get_metrics(&self) -> CacheMetrics {
        self.metrics.read().await.clone()
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let metrics_data = self.metrics.read().await;

        let content_size = self.content_cache.entry_count();
        let robots_size = self.robots_cache.entry_count();
        let redirect_size = self.redirect_cache.entry_count();

        CacheStats {
            content_size,
            robots_size,
            redirect_size,
            content_hits: metrics_data.content_hits,
            content_misses: metrics_data.content_misses,
            robots_hits: metrics_data.robots_hits,
            robots_misses: metrics_data.robots_misses,
            redirect_hits: metrics_data.redirect_hits,
            redirect_misses: metrics_data.redirect_misses,
        }
    }

    /// Clear all caches
    pub async fn clear_all(&self) {
        self.content_cache.invalidate_all();
        self.robots_cache.invalidate_all();
        self.redirect_cache.invalidate_all();
        info!("All caches cleared");
    }

    /// Clear content cache only
    pub async fn clear_content(&self) {
        self.content_cache.invalidate_all();
        info!("Content cache cleared");
    }

    /// Clear robots cache only
    pub async fn clear_robots(&self) {
        self.robots_cache.invalidate_all();
        info!("Robots cache cleared");
    }

    /// Clear redirect cache only
    pub async fn clear_redirect(&self) {
        self.redirect_cache.invalidate_all();
        info!("Redirect cache cleared");
    }
}

impl Default for CacheLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub content_size: u64,
    pub robots_size: u64,
    pub redirect_size: u64,
    pub content_hits: u64,
    pub content_misses: u64,
    pub robots_hits: u64,
    pub robots_misses: u64,
    pub redirect_hits: u64,
    pub redirect_misses: u64,
}

impl CacheStats {
    /// Calculate content cache hit rate
    pub fn content_hit_rate(&self) -> f64 {
        let total = self.content_hits + self.content_misses;
        if total == 0 {
            0.0
        } else {
            self.content_hits as f64 / total as f64
        }
    }

    /// Calculate robots cache hit rate
    pub fn robots_hit_rate(&self) -> f64 {
        let total = self.robots_hits + self.robots_misses;
        if total == 0 {
            0.0
        } else {
            self.robots_hits as f64 / total as f64
        }
    }

    /// Calculate redirect cache hit rate
    pub fn redirect_hit_rate(&self) -> f64 {
        let total = self.redirect_hits + self.redirect_misses;
        if total == 0 {
            0.0
        } else {
            self.redirect_hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_generation() {
        let url = "https://example.com";
        let headers1 = vec![("User-Agent".to_string(), "test".to_string())];
        let headers2 = vec![("User-Agent".to_string(), "test".to_string())];
        let headers3 = vec![("User-Agent".to_string(), "different".to_string())];

        let key1 = CacheLayer::generate_cache_key(url, Some(&headers1));
        let key2 = CacheLayer::generate_cache_key(url, Some(&headers2));
        let key3 = CacheLayer::generate_cache_key(url, Some(&headers3));
        let key_no_headers = CacheLayer::generate_cache_key(url, None);

        assert_eq!(key1, key2, "Same URL and headers should produce same key");
        assert_ne!(
            key1, key3,
            "Different headers should produce different keys"
        );
        assert_ne!(
            key1, key_no_headers,
            "With and without headers should differ"
        );
    }

    #[tokio::test]
    async fn test_cache_layer_creation() {
        let cache = CacheLayer::new();
        let stats = cache.get_stats().await;

        assert_eq!(stats.content_size, 0);
        assert_eq!(stats.robots_size, 0);
        assert_eq!(stats.redirect_size, 0);
    }

    #[tokio::test]
    async fn test_content_caching() {
        let cache = CacheLayer::new();
        let url = "https://example.com";

        let mut fetch_count = 0;

        // First fetch - should miss cache
        let _content1 = cache
            .get_or_fetch_content(url, None, || async {
                fetch_count += 1;
                Ok(CachedContent {
                    html: "<html></html>".to_string(),
                    status_code: 200,
                    content_type: Some("text/html".to_string()),
                    headers: vec![],
                    cached_at: 0,
                })
            })
            .await
            .unwrap();

        assert_eq!(fetch_count, 1);

        // Second fetch - should hit cache
        let _content2 = cache
            .get_or_fetch_content(url, None, || async {
                fetch_count += 1;
                Ok(CachedContent {
                    html: "<html></html>".to_string(),
                    status_code: 200,
                    content_type: Some("text/html".to_string()),
                    headers: vec![],
                    cached_at: 0,
                })
            })
            .await
            .unwrap();

        assert_eq!(
            fetch_count, 1,
            "Fetch function should not be called on cache hit"
        );

        let stats = cache.get_stats().await;
        assert_eq!(stats.content_hits, 1);
        assert_eq!(stats.content_misses, 1);
    }
}
