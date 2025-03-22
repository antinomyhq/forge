//! Retry utilities for handling transient errors

use std::future::Future;

use anyhow::Context as _;
use tokio::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for retry behavior with exponential backoff
#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay in milliseconds before first retry
    pub initial_delay_ms: u64,
    /// Multiplier for exponential backoff between retries
    pub backoff_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 200, // Start with 200ms delay
            backoff_factor: 2.0,   // Double the delay for each retry
        }
    }
}

impl RetryConfig {
    /// Calculate the delay duration for a specific retry attempt
    pub fn calculate_delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = self.backoff_factor.powi(attempt as i32 - 1) as u64;
        Duration::from_millis(self.initial_delay_ms * multiplier)
    }
}

/// Retry an operation with exponential backoff
pub async fn retry_with_backoff<F, Fut, T>(
    operation_name: &str,
    operation: F,
    config: RetryConfig,
    should_retry: fn(&anyhow::Error) -> bool,
) -> anyhow::Result<T>
where
    F: Fn() -> Fut + Clone,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let mut attempt = 1;

    loop {
        let operation_attempt = operation.clone();
        match operation_attempt().await {
            Ok(result) => {
                if attempt > 1 {
                    info!(
                        operation = %operation_name,
                        attempts = attempt,
                        "Operation succeeded after {} retries",
                        attempt - 1
                    );
                }
                return Ok(result);
            }
            Err(err) => {
                let is_retriable = should_retry(&err);

                // Check if we've reached max attempts or error is not retryable
                if attempt >= config.max_attempts || !is_retriable {
                    if attempt > 1 {
                        warn!(
                            operation = %operation_name,
                            attempts = attempt,
                            error = ?err,
                            retryable = is_retriable,
                            "Operation failed after {} retries (max: {})",
                            attempt - 1,
                            config.max_attempts
                        );
                    } else if !is_retriable {
                        // Log differently for errors that aren't retried at all
                        debug!(
                            operation = %operation_name,
                            error = ?err,
                            "Operation failed with non-retryable error"
                        );
                    }
                    return Err(err)
                        .with_context(|| format!("Operation '{}' failed", operation_name));
                }

                // Calculate backoff delay and prepare for retry
                let delay = config.calculate_delay_for_attempt(attempt);
                debug!(
                    operation = %operation_name,
                    attempt = attempt,
                    max_attempts = config.max_attempts,
                    delay_ms = %delay.as_millis(),
                    error = ?err,
                    "Retrying operation after delay"
                );

                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use tokio::time::sleep;

    use super::*;
    use crate::{AgentId, Error as DomainError};

    // Helper function that fails with a retryable error, then succeeds
    async fn retryable_operation(
        counter: &Arc<AtomicU32>,
        max_fails: u32,
        retryable: bool,
    ) -> anyhow::Result<String> {
        let current = counter.fetch_add(1, Ordering::SeqCst);

        if current < max_fails {
            // Sleep to simulate work being done
            sleep(Duration::from_millis(10)).await;

            // Return different types of errors based on the retryable flag
            if retryable {
                // Return a retryable error (ToolCallParse)
                Err(anyhow::Error::new(DomainError::ToolCallParse(
                    "Test failure".to_string(),
                )))
            } else {
                // Return a non-retryable error (AgentUndefined)
                Err(anyhow::Error::new(DomainError::AgentUndefined(
                    AgentId::new("agent1"),
                )))
            }
        } else {
            Ok("Success".to_string())
        }
    }

    // Simple retry predicate for testing - only retry ToolCallParse errors
    fn is_tool_call_parse_error(err: &anyhow::Error) -> bool {
        if let Some(domain_err) = err.downcast_ref::<DomainError>() {
            return matches!(domain_err, DomainError::ToolCallParse(_));
        }
        false
    }

    #[test]
    fn test_retry_config() {
        let config = RetryConfig::default();

        // First attempt delay should be initial_delay_ms
        assert_eq!(
            config.calculate_delay_for_attempt(1),
            Duration::from_millis(200),
            "First attempt delay should be initial_delay_ms"
        );

        // Second attempt delay should be initial_delay_ms * backoff_factor
        assert_eq!(
            config.calculate_delay_for_attempt(2),
            Duration::from_millis(400),
            "Second attempt delay should be initial_delay_ms * backoff_factor"
        );

        // Third attempt delay should be initial_delay_ms * backoff_factor^2
        assert_eq!(
            config.calculate_delay_for_attempt(3),
            Duration::from_millis(800),
            "Third attempt delay should be initial_delay_ms * backoff_factor^2"
        );
    }

    #[tokio::test]
    async fn test_retry_with_retryable_error() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_backoff(
            "test_operation",
            move || {
                let counter = counter_clone.clone();
                async move { retryable_operation(&counter, 2, true).await }
            },
            RetryConfig::default(),
            is_tool_call_parse_error,
        )
        .await;

        assert!(result.is_ok(), "Operation should succeed after retries");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            3,
            "Should execute 3 times (initial + 2 retries)"
        );
    }

    #[tokio::test]
    async fn test_no_retry_for_non_retryable_error() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_backoff(
            "test_operation",
            move || {
                let counter = counter_clone.clone();
                async move { retryable_operation(&counter, 2, false).await }
            },
            RetryConfig::default(),
            is_tool_call_parse_error,
        )
        .await;

        assert!(result.is_err(), "Operation should fail without retries");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "Should execute only once without retries"
        );
    }

    #[tokio::test]
    async fn test_retry_config_custom() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        // Custom config with 5 max attempts
        let config = RetryConfig { max_attempts: 5, initial_delay_ms: 10, backoff_factor: 1.5 };

        let result = retry_with_backoff(
            "test_custom_config",
            move || {
                let counter = counter_clone.clone();
                async move { retryable_operation(&counter, 4, true).await }
            },
            config,
            is_tool_call_parse_error,
        )
        .await;

        assert!(
            result.is_ok(),
            "Operation should succeed with custom retry config"
        );
        assert_eq!(
            counter.load(Ordering::SeqCst),
            5,
            "Should execute 5 times (initial + 4 retries)"
        );
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_backoff(
            "test_max_retries",
            move || {
                let counter = counter_clone.clone();
                // This will always fail with retryable errors
                async move { retryable_operation(&counter, 10, true).await }
            },
            RetryConfig::default(), // Default is 3 max attempts
            is_tool_call_parse_error,
        )
        .await;

        assert!(result.is_err(), "Operation should fail after max retries");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            3,
            "Should execute 3 times (initial + 2 retries)"
        );
    }
}
