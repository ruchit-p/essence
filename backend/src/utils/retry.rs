use backoff::{ExponentialBackoff, ExponentialBackoffBuilder};
use std::time::Duration;
use tracing::{debug, warn};

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_interval: Duration,
    pub max_interval: Duration,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_interval: Duration::from_millis(500),
            max_interval: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    pub fn to_backoff(&self) -> ExponentialBackoff {
        ExponentialBackoffBuilder::new()
            .with_initial_interval(self.initial_interval)
            .with_max_interval(self.max_interval)
            .with_multiplier(self.multiplier)
            .with_max_elapsed_time(Some(Duration::from_secs(60)))
            .build()
    }
}

/// Retry a fallible async operation with exponential backoff
pub async fn retry_with_backoff<F, Fut, T, E>(operation: F, config: &RetryConfig) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = config.initial_interval;
    let mut attempt = 0;

    loop {
        attempt += 1;

        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    debug!("Operation succeeded on attempt {}", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= config.max_retries {
                    warn!("Operation failed after {} attempts: {}", attempt, e);
                    return Err(e);
                }

                warn!(
                    "Operation failed (attempt {}): {}. Retrying in {:?}...",
                    attempt, e, delay
                );

                // Wait before retrying
                tokio::time::sleep(delay).await;

                // Exponential backoff
                delay = std::cmp::min(
                    Duration::from_millis((delay.as_millis() as f64 * config.multiplier) as u64),
                    config.max_interval,
                );
            }
        }
    }
}

/// Different retry strategies for different scenarios
#[derive(Debug, Clone, Copy)]
pub enum RetryStrategy {
    Aggressive,   // 5 retries, quick backoff
    Standard,     // 3 retries, normal backoff
    Conservative, // 2 retries, long backoff
    None,         // No retries
}

impl RetryStrategy {
    pub fn to_config(&self) -> RetryConfig {
        match self {
            Self::Aggressive => RetryConfig {
                max_retries: 5,
                initial_interval: Duration::from_millis(200),
                max_interval: Duration::from_secs(10),
                multiplier: 1.5,
            },
            Self::Standard => RetryConfig::default(),
            Self::Conservative => RetryConfig {
                max_retries: 2,
                initial_interval: Duration::from_secs(2),
                max_interval: Duration::from_secs(60),
                multiplier: 3.0,
            },
            Self::None => RetryConfig {
                max_retries: 0,
                initial_interval: Duration::from_millis(0),
                max_interval: Duration::from_millis(0),
                multiplier: 1.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_eventually_succeeds() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result = retry_with_backoff(
            move || {
                let attempt = attempt_clone.clone();
                async move {
                    let count = attempt.fetch_add(1, Ordering::SeqCst) + 1;
                    if count < 3 {
                        Err("Temporary failure")
                    } else {
                        Ok("Success")
                    }
                }
            },
            &RetryConfig::default(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success");
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_respects_max_attempts() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result = retry_with_backoff(
            move || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>("Always fails")
                }
            },
            &RetryConfig {
                max_retries: 3,
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_exponential_backoff_timing() {
        let start = std::time::Instant::now();

        let _ = retry_with_backoff(
            || async { Err::<(), _>("Fail") },
            &RetryConfig {
                max_retries: 3,
                initial_interval: Duration::from_millis(100),
                multiplier: 2.0,
                ..Default::default()
            },
        )
        .await;

        let elapsed = start.elapsed();

        // Should wait approximately: 100ms + 200ms = 300ms (2 retries after first failure)
        // Allow some variance for system scheduling
        assert!(
            elapsed.as_millis() >= 200,
            "Expected at least 200ms, got {}ms",
            elapsed.as_millis()
        );
    }

    #[tokio::test]
    async fn test_retry_strategy_aggressive() {
        let config = RetryStrategy::Aggressive.to_config();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_interval, Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_retry_strategy_standard() {
        let config = RetryStrategy::Standard.to_config();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_interval, Duration::from_millis(500));
    }

    #[tokio::test]
    async fn test_retry_strategy_conservative() {
        let config = RetryStrategy::Conservative.to_config();
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.initial_interval, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn test_retry_strategy_none() {
        let config = RetryStrategy::None.to_config();
        assert_eq!(config.max_retries, 0);
    }

    #[tokio::test]
    async fn test_no_retry_on_immediate_success() {
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();

        let result = retry_with_backoff(
            move || {
                let attempt = attempt_clone.clone();
                async move {
                    attempt.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, String>("Success")
                }
            },
            &RetryConfig::default(),
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(attempt.load(Ordering::SeqCst), 1); // Should succeed on first attempt
    }
}
