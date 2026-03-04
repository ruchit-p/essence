use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Simple circuit breaker for testing
struct CircuitBreaker {
    state: Arc<Mutex<CircuitState>>,
    failure_count: Arc<Mutex<usize>>,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
    failure_threshold: usize,
    timeout: Duration,
}

impl CircuitBreaker {
    fn new(failure_threshold: usize, timeout: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitState::Closed)),
            failure_count: Arc::new(Mutex::new(0)),
            last_failure_time: Arc::new(Mutex::new(None)),
            failure_threshold,
            timeout,
        }
    }

    fn get_state(&self) -> CircuitState {
        let state = self.state.lock().unwrap();
        state.clone()
    }

    fn is_open(&self) -> bool {
        let mut state = self.state.lock().unwrap();

        if *state == CircuitState::Open {
            // Check if timeout has elapsed
            let last_failure = self.last_failure_time.lock().unwrap();
            if let Some(time) = *last_failure {
                if Instant::now().duration_since(time) >= self.timeout {
                    *state = CircuitState::HalfOpen;
                    return false;
                }
            }
            return true;
        }

        false
    }

    fn record_success(&self) {
        let mut state = self.state.lock().unwrap();
        let mut failure_count = self.failure_count.lock().unwrap();

        *failure_count = 0;
        *state = CircuitState::Closed;
    }

    fn record_failure(&self) {
        let mut state = self.state.lock().unwrap();
        let mut failure_count = self.failure_count.lock().unwrap();
        let mut last_failure_time = self.last_failure_time.lock().unwrap();

        *failure_count += 1;
        *last_failure_time = Some(Instant::now());

        if *failure_count >= self.failure_threshold {
            *state = CircuitState::Open;
        }
    }

    fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        let mut failure_count = self.failure_count.lock().unwrap();
        let mut last_failure_time = self.last_failure_time.lock().unwrap();

        *state = CircuitState::Closed;
        *failure_count = 0;
        *last_failure_time = None;
    }
}

#[tokio::test]
async fn test_circuit_breaker_starts_closed() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(5));
    assert_eq!(cb.get_state(), CircuitState::Closed);
    assert!(!cb.is_open());
}

#[tokio::test]
async fn test_circuit_breaker_opens_on_failures() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(5));

    // Record failures below threshold
    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Closed);

    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Closed);

    // Third failure should open the circuit
    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Open);
    assert!(cb.is_open());
}

#[tokio::test]
async fn test_circuit_breaker_resets_on_success() {
    let cb = CircuitBreaker::new(3, Duration::from_secs(5));

    // Record some failures
    cb.record_failure();
    cb.record_failure();

    // Success should reset failure count
    cb.record_success();
    assert_eq!(cb.get_state(), CircuitState::Closed);

    // Should be able to fail twice more before opening
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Closed);

    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Open);
}

#[tokio::test]
async fn test_circuit_breaker_half_open_after_timeout() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(100));

    // Open the circuit
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Open);

    // Should still be open immediately
    assert!(cb.is_open());

    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should transition to half-open
    assert!(!cb.is_open());
    assert_eq!(cb.get_state(), CircuitState::HalfOpen);
}

#[tokio::test]
async fn test_circuit_breaker_half_open_to_closed() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(100));

    // Open the circuit
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Open);

    // Wait for timeout to get to half-open
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!cb.is_open());
    assert_eq!(cb.get_state(), CircuitState::HalfOpen);

    // Success in half-open should close the circuit
    cb.record_success();
    assert_eq!(cb.get_state(), CircuitState::Closed);
}

#[tokio::test]
async fn test_circuit_breaker_half_open_to_open() {
    let cb = CircuitBreaker::new(2, Duration::from_millis(100));

    // Open the circuit
    cb.record_failure();
    cb.record_failure();

    // Wait for timeout to get to half-open
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(cb.get_state(), CircuitState::HalfOpen);

    // Failure in half-open should immediately reopen
    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Open);
}

#[tokio::test]
async fn test_circuit_breaker_reset() {
    let cb = CircuitBreaker::new(2, Duration::from_secs(5));

    // Open the circuit
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.get_state(), CircuitState::Open);

    // Manual reset
    cb.reset();
    assert_eq!(cb.get_state(), CircuitState::Closed);
    assert!(!cb.is_open());
}

#[tokio::test]
async fn test_circuit_breaker_with_different_thresholds() {
    let cb_low = CircuitBreaker::new(1, Duration::from_secs(5));
    let cb_high = CircuitBreaker::new(5, Duration::from_secs(5));

    // Low threshold should open after 1 failure
    cb_low.record_failure();
    assert_eq!(cb_low.get_state(), CircuitState::Open);

    // High threshold should remain closed after 1 failure
    cb_high.record_failure();
    assert_eq!(cb_high.get_state(), CircuitState::Closed);

    // Should need 5 failures to open
    for _ in 0..4 {
        cb_high.record_failure();
    }
    assert_eq!(cb_high.get_state(), CircuitState::Open);
}

#[tokio::test]
async fn test_circuit_breaker_timeout_variations() {
    let cb_short = CircuitBreaker::new(1, Duration::from_millis(50));
    let cb_long = CircuitBreaker::new(1, Duration::from_secs(10));

    // Open both circuits
    cb_short.record_failure();
    cb_long.record_failure();

    assert!(cb_short.is_open());
    assert!(cb_long.is_open());

    // Wait for short timeout
    tokio::time::sleep(Duration::from_millis(75)).await;

    // Short should be half-open, long should still be open
    assert!(!cb_short.is_open());
    assert!(cb_long.is_open());
}

#[tokio::test]
async fn test_circuit_breaker_prevents_calls_when_open() {
    let cb = CircuitBreaker::new(2, Duration::from_secs(5));

    // Simulate making calls through circuit breaker
    let mut successful_calls = 0;
    let mut rejected_calls = 0;

    // Make some calls that fail
    for _ in 0..2 {
        if !cb.is_open() {
            // Simulate failed call
            cb.record_failure();
        } else {
            rejected_calls += 1;
        }
    }

    // Circuit should now be open
    assert!(cb.is_open());

    // Additional calls should be rejected
    for _ in 0..5 {
        if !cb.is_open() {
            successful_calls += 1;
        } else {
            rejected_calls += 1;
        }
    }

    assert_eq!(successful_calls, 0);
    assert_eq!(rejected_calls, 5);
}
