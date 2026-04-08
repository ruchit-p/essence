//! DNS caching for improved latency
//!
//! This module provides DNS caching using hickory-resolver (formerly trust-dns)
//! to reduce DNS lookup latency by 10-50ms. Implements LRU eviction to prevent
//! unbounded memory growth.

use crate::error::{Result, ScrapeError};
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::TokioAsyncResolver;
use lru::LruCache;
use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, trace};

/// DNS cache with LRU eviction
///
/// Caches DNS lookups to reduce latency. Thread-safe via Arc<Mutex<...>>.
#[derive(Clone)]
pub struct DnsCache {
    resolver: Arc<TokioAsyncResolver>,
    cache: Arc<Mutex<LruCache<String, Vec<IpAddr>>>>,
    stats: Arc<Mutex<CacheStats>>,
}

#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub lookups: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        if self.lookups == 0 {
            0.0
        } else {
            self.hits as f64 / self.lookups as f64
        }
    }
}

impl DnsCache {
    /// Create a new DNS cache with default capacity (1000 entries)
    pub fn new() -> Result<Self> {
        Self::with_capacity(1000)
    }

    /// Create a new DNS cache with specified capacity
    pub fn with_capacity(capacity: usize) -> Result<Self> {
        // Use system DNS configuration
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

        Ok(Self {
            resolver: Arc::new(resolver),
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1000).unwrap()),
            ))),
            stats: Arc::new(Mutex::new(CacheStats::default())),
        })
    }

    /// Lookup a domain, using cache if available
    ///
    /// Returns a list of IP addresses for the domain. The first IP is typically
    /// the preferred address for connection.
    pub async fn lookup(&self, domain: &str) -> Result<Vec<IpAddr>> {
        // Update stats
        {
            let mut stats = self.stats.lock().await;
            stats.lookups += 1;
        }

        // Check cache first
        {
            let mut cache = self.cache.lock().await;
            if let Some(ips) = cache.get(domain) {
                trace!("DNS cache hit for domain: {}", domain);
                let mut stats = self.stats.lock().await;
                stats.hits += 1;
                return Ok(ips.clone());
            }
        }

        // Cache miss - perform actual DNS lookup
        debug!("DNS cache miss for domain: {}, performing lookup", domain);
        {
            let mut stats = self.stats.lock().await;
            stats.misses += 1;
        }

        let response = self.resolver.lookup_ip(domain).await.map_err(|e| {
            ScrapeError::Internal(format!("DNS lookup failed for {}: {}", domain, e))
        })?;

        let ips: Vec<IpAddr> = response.iter().collect();

        if ips.is_empty() {
            return Err(ScrapeError::Internal(format!(
                "No IP addresses found for domain: {}",
                domain
            )));
        }

        debug!("DNS lookup resolved {} to {} addresses", domain, ips.len());

        // Store in cache
        {
            let mut cache = self.cache.lock().await;
            cache.put(domain.to_string(), ips.clone());
        }

        Ok(ips)
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        self.stats.lock().await.clone()
    }

    /// Reset cache statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.lock().await;
        *stats = CacheStats::default();
    }

    /// Clear the cache
    pub async fn clear(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
        debug!("DNS cache cleared");
    }

    /// Get current cache size
    pub async fn size(&self) -> usize {
        let cache = self.cache.lock().await;
        cache.len()
    }
}

impl Default for DnsCache {
    fn default() -> Self {
        Self::new().expect("Failed to create default DNS cache")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dns_cache_creation() {
        let cache = DnsCache::new();
        assert!(cache.is_ok());
    }

    #[tokio::test]
    async fn test_dns_cache_with_capacity() {
        let cache = DnsCache::with_capacity(500);
        assert!(cache.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_dns_lookup_success() {
        let cache = DnsCache::new().unwrap();
        let result = cache.lookup("google.com").await;
        assert!(result.is_ok());
        let ips = result.unwrap();
        assert!(!ips.is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_dns_cache_hit() {
        let cache = DnsCache::new().unwrap();

        // First lookup - cache miss
        let result1 = cache.lookup("google.com").await;
        assert!(result1.is_ok());

        // Second lookup - should be cache hit
        let result2 = cache.lookup("google.com").await;
        assert!(result2.is_ok());

        // Verify they return the same IPs
        assert_eq!(result1.unwrap(), result2.unwrap());

        // Check stats
        let stats = cache.stats().await;
        assert_eq!(stats.lookups, 2);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate(), 0.5);
    }

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_dns_cache_multiple_domains() {
        let cache = DnsCache::new().unwrap();

        // Lookup multiple domains
        let _ = cache.lookup("google.com").await;
        let _ = cache.lookup("github.com").await;
        let _ = cache.lookup("google.com").await; // cache hit
        let _ = cache.lookup("github.com").await; // cache hit

        let stats = cache.stats().await;
        assert_eq!(stats.lookups, 4);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 2);
    }

    #[tokio::test]
    async fn test_dns_cache_stats() {
        let cache = DnsCache::new().unwrap();
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.lookups, 0);
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[tokio::test]
    async fn test_dns_cache_clear() {
        let cache = DnsCache::new().unwrap();
        cache.clear().await;
        let size = cache.size().await;
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn test_dns_cache_reset_stats() {
        let cache = DnsCache::new().unwrap();

        // Manually update stats
        {
            let mut stats = cache.stats.lock().await;
            stats.hits = 10;
            stats.misses = 5;
            stats.lookups = 15;
        }

        // Reset
        cache.reset_stats().await;

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.lookups, 0);
    }

    #[tokio::test]
    async fn test_invalid_domain() {
        let cache = DnsCache::new().unwrap();
        let result = cache
            .lookup("invalid.domain.that.does.not.exist.xyz123")
            .await;
        assert!(result.is_err());
    }
}
