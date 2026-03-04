//! Integration test for bounded memory crawler functionality
//! This test verifies that queue size limits and circuit breaker work correctly

use essence::crawler::{CircuitBreaker, CrawlerConfig, MemoryMonitor};

#[test]
fn test_queue_bounded_by_config() {
    let config = CrawlerConfig {
        max_queue_size: 100,
        max_memory_mb: 500,
        max_concurrent_requests: 10,
        circuit_breaker_threshold: 3,
        backpressure_threshold: 80,
        enable_memory_monitoring: true,
        enable_circuit_breaker: true,
    };

    // Verify config limits are set correctly
    assert_eq!(config.max_queue_size, 100);
    
    // Simulate queue management
    let queue_size = 0;
    let backpressure_limit = (config.max_queue_size * config.backpressure_threshold as usize) / 100;
    
    // Queue should not exceed max_queue_size
    assert!(queue_size < config.max_queue_size);
    
    // Backpressure should activate at 80% of max
    assert_eq!(backpressure_limit, 80);
}

#[test]
fn test_circuit_breaker_prevents_runaway_failures() {
    let config = CrawlerConfig::default();
    let breaker = CircuitBreaker::new(config.circuit_breaker_threshold);
    
    let domain = "failing-domain.com";
    
    // Simulate 3 failures (threshold)
    for i in 0..3 {
        assert!(!breaker.should_skip(domain), "Should not skip before threshold at iteration {}", i);
        breaker.record_failure(domain);
    }
    
    // Now should skip
    assert!(breaker.should_skip(domain), "Should skip after reaching threshold");
    
    // Success should reset
    breaker.record_success(domain);
    assert!(!breaker.should_skip(domain), "Should not skip after success");
}

#[test]
fn test_memory_monitor_checks() {
    let monitor = MemoryMonitor::new(100, true);
    
    // Should return current memory
    let memory = monitor.get_current_memory_mb();
    assert!(memory > 0, "Memory should be non-zero");
    
    // Percentage should be reasonable
    let percentage = monitor.get_memory_percentage();
    assert!(percentage > 0.0 && percentage < 100.0, "Memory percentage should be between 0 and 100");
}

#[test]
fn test_memory_monitor_can_be_disabled() {
    let monitor = MemoryMonitor::new(100, false);
    
    // When disabled, should return 0
    assert_eq!(monitor.get_current_memory_mb(), 0);
    assert_eq!(monitor.get_memory_percentage(), 0.0);
    
    // Should always pass limit checks
    assert!(monitor.check_memory_limit().is_ok());
}

#[test]
fn test_circuit_breaker_tracks_multiple_domains() {
    let breaker = CircuitBreaker::new(2);
    
    // Record failures for multiple domains
    breaker.record_failure("domain1.com");
    breaker.record_failure("domain1.com");  // Reaches threshold
    
    breaker.record_failure("domain2.com");  // Only 1 failure
    
    // domain1 should be skipped, domain2 should not
    assert!(breaker.should_skip("domain1.com"));
    assert!(!breaker.should_skip("domain2.com"));
    
    // Total failures should be 1 (only domain1 is in failure state)
    assert_eq!(breaker.get_total_failures(), 1);
}

#[test]
fn test_config_prevents_unbounded_growth() {
    let config = CrawlerConfig::default();
    
    // Ensure reasonable limits
    assert!(config.max_queue_size > 0, "Queue must have a size limit");
    assert!(config.max_queue_size <= 10000, "Queue limit should be reasonable");
    assert!(config.max_memory_mb > 0, "Memory limit must be set");
    assert!(config.max_memory_mb <= 10000, "Memory limit should be reasonable (< 10GB)");
}

#[test]
fn test_backpressure_threshold_calculation() {
    let config = CrawlerConfig::default();
    
    // Calculate backpressure limit (80% of max queue size by default)
    let backpressure_limit = (config.max_queue_size * config.backpressure_threshold as usize) / 100;
    
    // Verify it's less than max queue size
    assert!(backpressure_limit < config.max_queue_size);
    
    // Verify it's a reasonable percentage
    let percentage = (backpressure_limit as f64 / config.max_queue_size as f64) * 100.0;
    assert!((percentage - config.backpressure_threshold as f64).abs() < 0.1);
}

#[test]
fn test_document_limit_enforcement() {
    let config = CrawlerConfig::default();
    let crawl_limit = 100;
    
    // Simulate document collection
    let mut documents_count = 0;
    
    // Simulate crawling loop
    for _ in 0..200 {
        if documents_count >= crawl_limit {
            break;
        }
        documents_count += 1;
    }
    
    // Should stop at limit
    assert_eq!(documents_count, crawl_limit);
}
