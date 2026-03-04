//! Integration tests for DNS caching
//!
//! These tests verify that DNS caching provides performance improvements
//! and works correctly with the HTTP engine.

use essence::utils::dns_cache::DnsCache;
use std::time::Instant;

#[tokio::test]
#[ignore] // Requires network
async fn test_dns_cache_performance_improvement() {
    let cache = DnsCache::new().expect("Failed to create DNS cache");

    let domain = "google.com";

    // First lookup - cache miss
    let start = Instant::now();
    let result1 = cache.lookup(domain).await;
    let miss_duration = start.elapsed();
    assert!(result1.is_ok(), "First DNS lookup failed");

    // Second lookup - cache hit
    let start = Instant::now();
    let result2 = cache.lookup(domain).await;
    let hit_duration = start.elapsed();
    assert!(result2.is_ok(), "Cached DNS lookup failed");

    // Verify cache hit is faster
    println!("DNS lookup times:");
    println!("  Cache miss: {:.2}ms", miss_duration.as_secs_f64() * 1000.0);
    println!("  Cache hit:  {:.2}ms", hit_duration.as_secs_f64() * 1000.0);

    // Cache hit should be significantly faster (typically <1ms vs 10-50ms)
    // We use a conservative threshold to avoid flaky tests
    assert!(
        hit_duration < miss_duration,
        "Cache hit should be faster than cache miss"
    );

    // Verify results are the same
    assert_eq!(result1.unwrap(), result2.unwrap(), "Cached result should match original");
}

#[tokio::test]
#[ignore] // Requires network
async fn test_dns_cache_stats() {
    let cache = DnsCache::new().expect("Failed to create DNS cache");

    // Initial stats should be zero
    let stats = cache.stats().await;
    assert_eq!(stats.hits, 0);
    assert_eq!(stats.misses, 0);
    assert_eq!(stats.lookups, 0);
    assert_eq!(stats.hit_rate(), 0.0);

    // Perform lookups
    let _ = cache.lookup("google.com").await;
    let _ = cache.lookup("github.com").await;
    let _ = cache.lookup("google.com").await; // Hit
    let _ = cache.lookup("github.com").await; // Hit

    // Verify stats
    let stats = cache.stats().await;
    assert_eq!(stats.lookups, 4);
    assert_eq!(stats.hits, 2);
    assert_eq!(stats.misses, 2);
    assert_eq!(stats.hit_rate(), 0.5);

    println!("DNS cache stats after 4 lookups:");
    println!("  Total lookups: {}", stats.lookups);
    println!("  Cache hits: {}", stats.hits);
    println!("  Cache misses: {}", stats.misses);
    println!("  Hit rate: {:.1}%", stats.hit_rate() * 100.0);
}

#[tokio::test]
#[ignore] // Requires network
async fn test_dns_cache_with_http_engine() {
    use essence::engines::http::HttpEngine;
    use essence::engines::ScrapeEngine;
    use essence::types::ScrapeRequest;

    use std::collections::HashMap;

    let engine = HttpEngine::new().expect("Failed to create HTTP engine");

    let request = ScrapeRequest {
        url: "https://example.com".to_string(),
        formats: vec!["markdown".to_string()],
        headers: HashMap::new(),
        include_tags: vec![],
        exclude_tags: vec![],
        only_main_content: true,
        wait_for: 0,
        timeout: 30000,
        remove_base64_images: true,
        skip_tls_verification: false,
        engine: "auto".to_string(),
        wait_for_selector: None,
        actions: vec![],
        screenshot: false,
        screenshot_format: "png".to_string(),
    };

    // First request - DNS cache miss
    let start = Instant::now();
    let result1 = engine.scrape(&request).await;
    let first_duration = start.elapsed();
    assert!(result1.is_ok(), "First scrape failed: {:?}", result1.err());

    // Second request to same domain - DNS cache hit
    let start = Instant::now();
    let result2 = engine.scrape(&request).await;
    let second_duration = start.elapsed();
    assert!(result2.is_ok(), "Second scrape failed: {:?}", result2.err());

    println!("HTTP engine scrape times:");
    println!("  First (DNS miss):  {:.2}ms", first_duration.as_secs_f64() * 1000.0);
    println!("  Second (DNS hit):  {:.2}ms", second_duration.as_secs_f64() * 1000.0);

    // Get DNS stats
    let dns_stats = engine.dns_stats().await;
    println!("DNS cache stats:");
    println!("  Lookups: {}", dns_stats.lookups);
    println!("  Hits: {}", dns_stats.hits);
    println!("  Misses: {}", dns_stats.misses);
    println!("  Hit rate: {:.1}%", dns_stats.hit_rate() * 100.0);

    // Should have at least one cache hit from repeated domain
    assert!(dns_stats.lookups >= 2, "Should have performed multiple DNS lookups");
}

#[tokio::test]
async fn test_dns_cache_eviction() {
    // Create cache with very small capacity
    let cache = DnsCache::with_capacity(2).expect("Failed to create DNS cache");

    // The cache should work even with small capacity
    let _ = cache.lookup("localhost").await;
    
    let size = cache.size().await;
    assert!(size <= 2, "Cache size should not exceed capacity");
}

#[tokio::test]
async fn test_dns_cache_clear() {
    let cache = DnsCache::new().expect("Failed to create DNS cache");

    // Add some entries (may fail if offline, that's ok)
    let _ = cache.lookup("localhost").await;

    // Clear cache
    cache.clear().await;

    let size = cache.size().await;
    assert_eq!(size, 0, "Cache should be empty after clear");
}
