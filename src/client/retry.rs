//! Retry logic with exponential backoff.
//!
//! This module provides the [`retry_with_backoff`] helper function used by
//! [`RpcClient`](crate::RpcClient) to handle transient failures in broker-based
//! transports where servers may not be subscribed when requests are published.
//!
//! # Retry Strategy
//!
//! - Only retries [`RpcError::TransportRetryable`](crate::RpcError::TransportRetryable) errors
//! - Uses exponential backoff with randomized jitter to prevent thundering herd
//! - Caps delay at `max_delay` to prevent excessive wait times
//! - Logs each retry attempt with timing information for debugging

use std::collections::hash_map::RandomState;
use std::future::Future;
use std::hash::{BuildHasher, Hash};
use std::time::Duration;
use tokio::time::sleep;

/// Retry an async operation with exponential backoff.
///
/// This function executes the provided operation and retries it according to the
/// retry configuration if it fails with a retryable error. Non-retryable errors
/// cause immediate failure.
///
/// # Backoff Algorithm
///
/// - First retry: `initial_delay` (with jitter)
/// - Subsequent retries: `min(current_delay * multiplier, max_delay)` (with jitter)
/// - Jitter: ±25% randomization to prevent synchronized retries
///
/// # Arguments
///
/// - `config`: RPC configuration containing optional retry settings
/// - `operation`: Async closure that returns `Result<T>`
///
/// # Returns
///
/// - `Ok(T)` if the operation succeeds (on any attempt)
/// - `Err(RpcError)` if all retry attempts are exhausted or a non-retryable error occurs
///
/// # Example
///
/// ```ignore
/// let result = retry_with_backoff(&config, || async {
///     client.request_to("server", "method", req).await
/// }).await?;
/// ```
pub(crate) async fn retry_with_backoff<F, Fut, T>(
    config: &crate::RpcConfig,
    mut operation: F,
) -> crate::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = crate::Result<T>>,
{
    let retry_config = match &config.retry_config {
        Some(cfg) => cfg,
        None => {
            // No retry configured, just execute once
            return operation().await;
        }
    };

    let mut attempt = 0;
    let mut current_delay = retry_config.initial_delay;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(crate::RpcError::TransportRetryable(details)) => {
                attempt += 1;

                // Check if we've exhausted retry attempts
                if attempt > retry_config.max_attempts {
                    crate::log_debug!(
                        "retry exhausted after {} attempts, last error: {}",
                        retry_config.max_attempts,
                        details
                    );
                    return Err(crate::RpcError::TransportRetryable(details));
                }

                // Apply jitter: ±25% randomization
                let jittered_delay = apply_jitter(current_delay);

                crate::log_debug!(
                    "retry attempt {}/{}, waiting {:?} before retry (error: {})",
                    attempt,
                    retry_config.max_attempts,
                    jittered_delay,
                    details
                );

                sleep(jittered_delay).await;

                // Calculate next delay with exponential backoff
                let next_delay = Duration::from_secs_f64(
                    current_delay.as_secs_f64() * retry_config.multiplier as f64,
                );
                current_delay = next_delay.min(retry_config.max_delay);
            }
            Err(err) => {
                // Non-retryable error, fail immediately
                return Err(err);
            }
        }
    }
}

/// Apply ±25% jitter to a duration to prevent thundering herd.
///
/// Uses a simple multiplicative jitter: `delay * (0.75 + random(0.0..0.5))`
fn apply_jitter(delay: Duration) -> Duration {
    // Generate pseudo-random value using thread-local randomness
    // We use RandomState which is cheap and good enough for jitter
    let random_state = RandomState::new();
    let mut hasher = random_state.build_hasher();

    // Hash current time for randomness
    std::time::SystemTime::now().hash(&mut hasher);
    let hash = random_state.hash_one(std::time::SystemTime::now());

    // Convert to 0.0..1.0 range
    let random_factor = (hash % 1000) as f64 / 1000.0;

    // Apply jitter: multiply delay by (0.75 + random_factor * 0.5)
    // This gives us a range of 0.75x to 1.25x the original delay
    let jitter_multiplier = 0.75 + (random_factor * 0.5);

    Duration::from_secs_f64(delay.as_secs_f64() * jitter_multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    /// Helper to create a test RpcConfig with retry settings
    fn test_config(retry_config: crate::RetryConfig) -> crate::RpcConfig {
        let mut config = crate::RpcConfig::memory("test-client");
        config.retry_config = Some(retry_config);
        config
    }

    #[tokio::test]
    async fn test_no_retry_on_success() {
        let config = test_config(crate::RetryConfig::default());
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(&config, || {
            let count = call_count_clone.clone();
            async move {
                let mut c = count.lock().unwrap();
                *c += 1;
                Ok::<i32, crate::RpcError>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(*call_count.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_retry_on_retryable_error() {
        let retry_config = crate::RetryConfig {
            max_attempts: 3,
            multiplier: 2.0,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
        };
        let config = test_config(retry_config);
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(&config, || {
            let count = call_count_clone.clone();
            async move {
                let mut c = count.lock().unwrap();
                *c += 1;
                let attempt = *c;
                drop(c);

                if attempt < 3 {
                    Err(crate::RpcError::TransportRetryable(
                        "simulated failure".into(),
                    ))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(*call_count.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_retry_exhaustion() {
        let retry_config = crate::RetryConfig {
            max_attempts: 2,
            multiplier: 2.0,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
        };
        let config = test_config(retry_config);
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(&config, || {
            let count = call_count_clone.clone();
            async move {
                let mut c = count.lock().unwrap();
                *c += 1;
                drop(c);
                Err::<i32, _>(crate::RpcError::TransportRetryable("always fails".into()))
            }
        })
        .await;

        assert!(matches!(
            result,
            Err(crate::RpcError::TransportRetryable(_))
        ));
        // Initial attempt + 2 retries = 3 total calls
        assert_eq!(*call_count.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_no_retry_on_non_retryable_error() {
        let config = test_config(crate::RetryConfig::default());
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(&config, || {
            let count = call_count_clone.clone();
            async move {
                let mut c = count.lock().unwrap();
                *c += 1;
                drop(c);
                Err::<i32, _>(crate::RpcError::Transport("non-retryable".into()))
            }
        })
        .await;

        assert!(matches!(result, Err(crate::RpcError::Transport(_))));
        assert_eq!(*call_count.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_exponential_backoff_timing() {
        let retry_config = crate::RetryConfig {
            max_attempts: 3,
            multiplier: 2.0,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(500),
        };
        let config = test_config(retry_config);
        let start = Instant::now();

        let _result = retry_with_backoff(&config, || async {
            Err::<i32, _>(crate::RpcError::TransportRetryable("test".into()))
        })
        .await;

        let elapsed = start.elapsed();

        // Expected delays (with jitter: 0.75x to 1.25x):
        // Attempt 1: 50ms * (0.75..1.25) = 37.5ms..62.5ms
        // Attempt 2: 100ms * (0.75..1.25) = 75ms..125ms
        // Attempt 3: 200ms * (0.75..1.25) = 150ms..250ms
        // Total min: ~262ms, Total max: ~437ms

        assert!(
            elapsed >= Duration::from_millis(200),
            "elapsed too short: {elapsed:?}",
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "elapsed too long: {elapsed:?}",
        );
    }

    #[tokio::test]
    async fn test_max_delay_cap() {
        let retry_config = crate::RetryConfig {
            max_attempts: 5,
            multiplier: 10.0, // Aggressive multiplier
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50), // Low cap
        };
        let config = test_config(retry_config);
        let start = Instant::now();

        let _result = retry_with_backoff(&config, || async {
            Err::<i32, _>(crate::RpcError::TransportRetryable("test".into()))
        })
        .await;

        let elapsed = start.elapsed();

        // Even with 10x multiplier, delays should be capped at 50ms
        // With jitter (0.75x-1.25x), max single delay is ~62ms
        // 5 retries * ~62ms = ~310ms max
        assert!(
            elapsed < Duration::from_millis(400),
            "max_delay cap not working: {elapsed:?}",
        );
    }

    #[test]
    fn test_jitter_range() {
        let delay = Duration::from_millis(100);

        // Test multiple times to ensure jitter stays in range
        for _ in 0..100 {
            let jittered = apply_jitter(delay);

            // Should be 75ms..125ms (±25%)
            assert!(
                jittered >= Duration::from_millis(75),
                "jitter too low: {jittered:?}",
            );
            assert!(
                jittered <= Duration::from_millis(125),
                "jitter too high: {jittered:?}",
            );
        }
    }
}
