use essence::crawler::{CircuitBreaker, CrawlerConfig, MemoryMonitor};

#[test]
fn test_crawler_config_defaults() {
    let config = CrawlerConfig::default();
    
    assert_eq!(config.max_queue_size, 1000, "Default max_queue_size should be 1000");
    assert_eq!(config.max_memory_mb, 500, "Default max_memory_mb should be 500");
    assert_eq!(config.max_concurrent_requests, 10, "Default max_concurrent_requests should be 10");
    assert_eq!(config.circuit_breaker_threshold, 3, "Default circuit_breaker_threshold should be 3");
    assert_eq!(config.backpressure_threshold, 80, "Default backpressure_threshold should be 80");
    assert!(config.enable_memory_monitoring, "Memory monitoring should be enabled by default");
    assert!(config.enable_circuit_breaker, "Circuit breaker should be enabled by default");
}

#[test]
fn test_circuit_breaker_threshold() {
    let breaker = CircuitBreaker::new(3);
    
    // Initially should not skip
    assert!(!breaker.should_skip("example.com"), "Should not skip initially");
    assert_eq!(breaker.get_failure_count("example.com"), 0);
    
    // Record failures up to threshold
    breaker.record_failure("example.com");
    assert_eq!(breaker.get_failure_count("example.com"), 1);
    assert!(!breaker.should_skip("example.com"), "Should not skip at 1 failure");
    
    breaker.record_failure("example.com");
    assert_eq!(breaker.get_failure_count("example.com"), 2);
    assert!(!breaker.should_skip("example.com"), "Should not skip at 2 failures");
    
    breaker.record_failure("example.com");
    assert_eq!(breaker.get_failure_count("example.com"), 3);
    assert!(breaker.should_skip("example.com"), "Should skip at 3 failures (threshold reached)");
    
    // Additional failures should still skip
    breaker.record_failure("example.com");
    assert_eq!(breaker.get_failure_count("example.com"), 4);
    assert!(breaker.should_skip("example.com"), "Should still skip at 4 failures");
}

#[test]
fn test_circuit_breaker_reset_on_success() {
    let breaker = CircuitBreaker::new(3);
    
    // Record failures
    breaker.record_failure("example.com");
    breaker.record_failure("example.com");
    assert_eq!(breaker.get_failure_count("example.com"), 2);
    
    // Record success should reset
    breaker.record_success("example.com");
    assert_eq!(breaker.get_failure_count("example.com"), 0);
    assert!(!breaker.should_skip("example.com"));
}

#[test]
fn test_circuit_breaker_multiple_domains() {
    let breaker = CircuitBreaker::new(2);
    
    // Record failures for multiple domains
    breaker.record_failure("domain1.com");
    breaker.record_failure("domain1.com");
    
    breaker.record_failure("domain2.com");
    breaker.record_failure("domain2.com");
    
    breaker.record_failure("domain3.com"); // Only 1 failure
    
    // Check skip status
    assert!(breaker.should_skip("domain1.com"), "domain1.com should be skipped");
    assert!(breaker.should_skip("domain2.com"), "domain2.com should be skipped");
    assert!(!breaker.should_skip("domain3.com"), "domain3.com should not be skipped");
    
    // Check total failures
    assert_eq!(breaker.get_total_failures(), 2, "Should have 2 domains in failure state");
    
    // Reset one domain
    breaker.record_success("domain1.com");
    assert!(!breaker.should_skip("domain1.com"), "domain1.com should not be skipped after reset");
    assert_eq!(breaker.get_total_failures(), 1, "Should have 1 domain in failure state");
}

#[test]
fn test_memory_monitor_disabled() {
    let monitor = MemoryMonitor::new(100, false);
    
    // Should always succeed when disabled
    assert!(monitor.check_memory_limit().is_ok(), "Should succeed when disabled");
    assert_eq!(monitor.get_current_memory_mb(), 0, "Should return 0 when disabled");
    assert_eq!(monitor.get_memory_percentage(), 0.0, "Should return 0.0 when disabled");
}

#[test]
fn test_memory_monitor_enabled() {
    let monitor = MemoryMonitor::new(100, true);
    
    // Should return actual memory usage (non-zero)
    let memory = monitor.get_current_memory_mb();
    assert!(memory > 0, "Should return non-zero memory when enabled");
    
    // Percentage should be calculated correctly
    let percentage = monitor.get_memory_percentage();
    assert!(percentage > 0.0, "Should return non-zero percentage when enabled");
}

#[test]
fn test_memory_monitor_high_limit() {
    // Set a very high limit that won't be exceeded
    let monitor = MemoryMonitor::new(100000, true);
    
    // Should not exceed limit
    assert!(monitor.check_memory_limit().is_ok(), "Should not exceed high limit");
}

#[test]
fn test_queue_size_calculations() {
    let config = CrawlerConfig::default();
    
    // Test backpressure threshold calculation
    let backpressure_limit = (config.max_queue_size * config.backpressure_threshold as usize) / 100;
    assert_eq!(backpressure_limit, 800, "Backpressure limit should be 80% of max_queue_size");
    
    // Verify max_queue_size is reasonable
    assert!(config.max_queue_size >= 100, "max_queue_size should be at least 100");
    assert!(config.max_queue_size <= 10000, "max_queue_size should not exceed 10000");
}

#[test]
fn test_circuit_breaker_isolation() {
    let breaker = CircuitBreaker::new(2);
    
    // Different domains should be isolated
    breaker.record_failure("domain1.com");
    breaker.record_failure("domain1.com");
    
    // domain1 should be skipped, domain2 should not
    assert!(breaker.should_skip("domain1.com"));
    assert!(!breaker.should_skip("domain2.com"));
    
    // Resetting domain1 should not affect domain2
    breaker.record_success("domain1.com");
    breaker.record_failure("domain2.com");
    
    assert!(!breaker.should_skip("domain1.com"));
    assert!(!breaker.should_skip("domain2.com"));
}

#[test]
fn test_config_serialization() {
    use serde_json;
    
    let config = CrawlerConfig::default();
    
    // Should serialize successfully
    let json = serde_json::to_string(&config).expect("Should serialize");
    assert!(json.contains("maxQueueSize"), "JSON should contain maxQueueSize in camelCase");
    
    // Should deserialize successfully
    let deserialized: CrawlerConfig = serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(deserialized.max_queue_size, config.max_queue_size);
    assert_eq!(deserialized.max_memory_mb, config.max_memory_mb);
}

#[test]
fn test_config_custom_values() {
    let json = r#"{
        "maxQueueSize": 500,
        "maxMemoryMb": 200,
        "maxConcurrentRequests": 5,
        "circuitBreakerThreshold": 5,
        "backpressureThreshold": 70,
        "enableMemoryMonitoring": false,
        "enableCircuitBreaker": false
    }"#;
    
    let config: CrawlerConfig = serde_json::from_str(json).expect("Should deserialize custom config");
    
    assert_eq!(config.max_queue_size, 500);
    assert_eq!(config.max_memory_mb, 200);
    assert_eq!(config.max_concurrent_requests, 5);
    assert_eq!(config.circuit_breaker_threshold, 5);
    assert_eq!(config.backpressure_threshold, 70);
    assert!(!config.enable_memory_monitoring);
    assert!(!config.enable_circuit_breaker);
}
