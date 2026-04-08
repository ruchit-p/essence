use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};

/// Simple in-memory cache for testing
#[derive(Clone)]
struct MemoryCache {
    data: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

struct CacheEntry {
    value: String,
    expires_at: Instant,
}

impl MemoryCache {
    fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        let mut cache = self.data.lock().unwrap();
        if let Some(entry) = cache.get(key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.value.clone());
            } else {
                cache.remove(key);
            }
        }
        None
    }

    fn set(&self, key: String, value: String, ttl: Duration) {
        let mut cache = self.data.lock().unwrap();
        cache.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    fn delete(&self, key: &str) {
        let mut cache = self.data.lock().unwrap();
        cache.remove(key);
    }

    fn clear(&self) {
        let mut cache = self.data.lock().unwrap();
        cache.clear();
    }

    fn len(&self) -> usize {
        let cache = self.data.lock().unwrap();
        cache.len()
    }
}

#[tokio::test]
async fn test_cache_hit() {
    let cache = MemoryCache::new();
    let key = "test_key".to_string();
    let value = "test_value".to_string();

    cache.set(key.clone(), value.clone(), Duration::from_secs(60));

    let result = cache.get(&key);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), value);
}

#[tokio::test]
async fn test_cache_miss() {
    let cache = MemoryCache::new();
    let result = cache.get("nonexistent_key");
    assert!(result.is_none());
}

#[tokio::test]
async fn test_cache_expiration() {
    let cache = MemoryCache::new();
    let key = "expiring_key".to_string();
    let value = "expiring_value".to_string();

    // Set with very short TTL
    cache.set(key.clone(), value, Duration::from_millis(100));

    // Should exist immediately
    assert!(cache.get(&key).is_some());

    // Wait for expiration
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should be gone
    assert!(cache.get(&key).is_none());
}

#[tokio::test]
async fn test_cache_update() {
    let cache = MemoryCache::new();
    let key = "update_key".to_string();

    cache.set(key.clone(), "value1".to_string(), Duration::from_secs(60));
    assert_eq!(cache.get(&key).unwrap(), "value1");

    cache.set(key.clone(), "value2".to_string(), Duration::from_secs(60));
    assert_eq!(cache.get(&key).unwrap(), "value2");
}

#[tokio::test]
async fn test_cache_delete() {
    let cache = MemoryCache::new();
    let key = "delete_key".to_string();
    let value = "delete_value".to_string();

    cache.set(key.clone(), value, Duration::from_secs(60));
    assert!(cache.get(&key).is_some());

    cache.delete(&key);
    assert!(cache.get(&key).is_none());
}

#[tokio::test]
async fn test_cache_clear() {
    let cache = MemoryCache::new();

    cache.set(
        "key1".to_string(),
        "value1".to_string(),
        Duration::from_secs(60),
    );
    cache.set(
        "key2".to_string(),
        "value2".to_string(),
        Duration::from_secs(60),
    );
    cache.set(
        "key3".to_string(),
        "value3".to_string(),
        Duration::from_secs(60),
    );

    assert_eq!(cache.len(), 3);

    cache.clear();
    assert_eq!(cache.len(), 0);
}

#[tokio::test]
async fn test_cache_concurrent_access() {
    let cache = MemoryCache::new();
    let mut handles = vec![];

    // Spawn multiple tasks writing to cache
    for i in 0..10 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            cache_clone.set(key, value, Duration::from_secs(60));
        });
        handles.push(handle);
    }

    // Wait for all writes
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all entries exist
    assert_eq!(cache.len(), 10);
    for i in 0..10 {
        let key = format!("key_{}", i);
        assert!(cache.get(&key).is_some());
    }
}

#[tokio::test]
async fn test_cache_with_different_ttls() {
    let cache = MemoryCache::new();

    cache.set(
        "short".to_string(),
        "short_value".to_string(),
        Duration::from_millis(100),
    );
    cache.set(
        "long".to_string(),
        "long_value".to_string(),
        Duration::from_secs(60),
    );

    // Both should exist initially
    assert!(cache.get("short").is_some());
    assert!(cache.get("long").is_some());

    // Wait for short TTL to expire
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Short should be gone, long should remain
    assert!(cache.get("short").is_none());
    assert!(cache.get("long").is_some());
}

#[tokio::test]
async fn test_cache_key_generation() {
    let cache = MemoryCache::new();

    // Test URL-based cache keys
    let url1 = "https://example.com/page1";
    let url2 = "https://example.com/page2";
    let url1_duplicate = "https://example.com/page1";

    let key1 = format!("url:{}", url1);
    let key2 = format!("url:{}", url2);
    let key1_dup = format!("url:{}", url1_duplicate);

    cache.set(
        key1.clone(),
        "content1".to_string(),
        Duration::from_secs(60),
    );
    cache.set(
        key2.clone(),
        "content2".to_string(),
        Duration::from_secs(60),
    );

    // Same URL should retrieve same content
    assert_eq!(cache.get(&key1_dup).unwrap(), "content1");
    assert_eq!(cache.get(&key2).unwrap(), "content2");
}

#[tokio::test]
async fn test_cache_eviction_on_expiry() {
    let cache = MemoryCache::new();

    // Add multiple entries with different expirations
    for i in 0..5 {
        let key = format!("key_{}", i);
        let ttl = Duration::from_millis(100 * (i + 1) as u64);
        cache.set(key, format!("value_{}", i), ttl);
    }

    assert_eq!(cache.len(), 5);

    // Wait for first entry to expire
    tokio::time::sleep(Duration::from_millis(150)).await;

    // First entry should be gone when accessed
    assert!(cache.get("key_0").is_none());

    // Others should still exist (not yet accessed)
    // Note: expiry is lazy - only checked on access
}
