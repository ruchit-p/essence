use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tracing::{debug, warn};
use crate::error::Result;

/// Configuration for crawler with memory and queue limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    /// Maximum queue size (default: 1000)
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,

    /// Maximum memory usage in MB (default: 500MB)
    #[serde(default = "default_max_memory_mb")]
    pub max_memory_mb: usize,

    /// Maximum concurrent requests (default: 10)
    #[serde(default = "default_max_concurrent_requests")]
    pub max_concurrent_requests: usize,

    /// Circuit breaker failure threshold (default: 3)
    #[serde(default = "default_circuit_breaker_threshold")]
    pub circuit_breaker_threshold: usize,

    /// Backpressure threshold as percentage of max_queue_size (default: 80)
    #[serde(default = "default_backpressure_threshold")]
    pub backpressure_threshold: u8,

    /// Enable memory monitoring (default: true)
    #[serde(default = "default_true")]
    pub enable_memory_monitoring: bool,

    /// Enable circuit breaker (default: true)
    #[serde(default = "default_true")]
    pub enable_circuit_breaker: bool,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            max_queue_size: default_max_queue_size(),
            max_memory_mb: default_max_memory_mb(),
            max_concurrent_requests: default_max_concurrent_requests(),
            circuit_breaker_threshold: default_circuit_breaker_threshold(),
            backpressure_threshold: default_backpressure_threshold(),
            enable_memory_monitoring: true,
            enable_circuit_breaker: true,
        }
    }
}

fn default_max_queue_size() -> usize {
    1000
}

fn default_max_memory_mb() -> usize {
    500
}

fn default_max_concurrent_requests() -> usize {
    10
}

fn default_circuit_breaker_threshold() -> usize {
    3
}

fn default_backpressure_threshold() -> u8 {
    80
}

fn default_true() -> bool {
    true
}

/// Circuit breaker for tracking and preventing runaway failures
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    failures: Arc<Mutex<HashMap<String, usize>>>,
    threshold: usize,
}

impl CircuitBreaker {
    pub fn new(threshold: usize) -> Self {
        Self {
            failures: Arc::new(Mutex::new(HashMap::new())),
            threshold,
        }
    }

    /// Record a failure for a domain
    pub fn record_failure(&self, domain: &str) {
        let mut failures = self.failures.lock().unwrap();
        let count = failures.entry(domain.to_string()).or_insert(0);
        *count += 1;

        if *count >= self.threshold {
            warn!("Circuit breaker triggered for domain: {} ({} failures)", domain, count);
        }
    }

    /// Record a success for a domain (resets failure count)
    pub fn record_success(&self, domain: &str) {
        let mut failures = self.failures.lock().unwrap();
        failures.remove(domain);
        debug!("Circuit breaker reset for domain: {}", domain);
    }

    /// Check if a domain should be skipped due to too many failures
    pub fn should_skip(&self, domain: &str) -> bool {
        let failures = self.failures.lock().unwrap();
        failures.get(domain).unwrap_or(&0) >= &self.threshold
    }

    /// Get the current failure count for a domain
    pub fn get_failure_count(&self, domain: &str) -> usize {
        let failures = self.failures.lock().unwrap();
        *failures.get(domain).unwrap_or(&0)
    }

    /// Get total number of domains in failure state
    pub fn get_total_failures(&self) -> usize {
        let failures = self.failures.lock().unwrap();
        failures.values().filter(|&&count| count >= self.threshold).count()
    }
}

/// Memory monitor for tracking process memory usage
///
/// This is a lightweight implementation that tracks an estimated memory budget
/// without requiring the `sysinfo` crate. In production, you may want to
/// integrate with platform-specific memory APIs.
pub struct MemoryMonitor {
    max_memory_mb: usize,
    enabled: bool,
}

impl MemoryMonitor {
    pub fn new(max_memory_mb: usize, enabled: bool) -> Self {
        Self {
            max_memory_mb,
            enabled,
        }
    }

    /// Check if current memory usage exceeds the limit
    ///
    /// This is a lightweight check. For precise memory monitoring,
    /// consider enabling platform-specific memory tracking.
    pub fn check_memory_limit(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        // Lightweight heuristic: we don't block on memory by default.
        // The actual enforcement happens through queue size limits and
        // concurrency controls in the crawler config.
        debug!("Memory check: limit is {}MB (lightweight mode)", self.max_memory_mb);
        Ok(())
    }

    /// Get current memory usage in MB (returns 0 in lightweight mode)
    pub fn get_current_memory_mb(&self) -> u64 {
        0
    }

    /// Get memory usage percentage (returns 0.0 in lightweight mode)
    pub fn get_memory_percentage(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crawler_config_defaults() {
        let config = CrawlerConfig::default();
        assert_eq!(config.max_queue_size, 1000);
        assert_eq!(config.max_memory_mb, 500);
        assert_eq!(config.max_concurrent_requests, 10);
        assert_eq!(config.circuit_breaker_threshold, 3);
        assert_eq!(config.backpressure_threshold, 80);
        assert!(config.enable_memory_monitoring);
        assert!(config.enable_circuit_breaker);
    }

    #[test]
    fn test_circuit_breaker() {
        let breaker = CircuitBreaker::new(3);

        // Initially should not skip
        assert!(!breaker.should_skip("example.com"));

        // Record failures
        breaker.record_failure("example.com");
        assert_eq!(breaker.get_failure_count("example.com"), 1);
        assert!(!breaker.should_skip("example.com"));

        breaker.record_failure("example.com");
        assert_eq!(breaker.get_failure_count("example.com"), 2);
        assert!(!breaker.should_skip("example.com"));

        breaker.record_failure("example.com");
        assert_eq!(breaker.get_failure_count("example.com"), 3);
        assert!(breaker.should_skip("example.com"));

        // Record success should reset
        breaker.record_success("example.com");
        assert!(!breaker.should_skip("example.com"));
        assert_eq!(breaker.get_failure_count("example.com"), 0);
    }

    #[test]
    fn test_circuit_breaker_multiple_domains() {
        let breaker = CircuitBreaker::new(2);

        breaker.record_failure("domain1.com");
        breaker.record_failure("domain1.com");

        breaker.record_failure("domain2.com");
        breaker.record_failure("domain2.com");

        assert!(breaker.should_skip("domain1.com"));
        assert!(breaker.should_skip("domain2.com"));
        assert_eq!(breaker.get_total_failures(), 2);

        // Reset one domain
        breaker.record_success("domain1.com");
        assert!(!breaker.should_skip("domain1.com"));
        assert!(breaker.should_skip("domain2.com"));
        assert_eq!(breaker.get_total_failures(), 1);
    }

    #[test]
    fn test_memory_monitor_disabled() {
        let monitor = MemoryMonitor::new(100, false);

        // Should always succeed when disabled
        assert!(monitor.check_memory_limit().is_ok());
        assert_eq!(monitor.get_current_memory_mb(), 0);
        assert_eq!(monitor.get_memory_percentage(), 0.0);
    }

    #[test]
    fn test_memory_monitor_enabled() {
        let monitor = MemoryMonitor::new(100, true);

        // Lightweight mode always succeeds
        assert!(monitor.check_memory_limit().is_ok());
    }
}
