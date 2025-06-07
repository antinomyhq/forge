use std::future::Future;
use std::time::Duration;

use anyhow::Context;
use backon::{ExponentialBuilder, Retryable};
use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters, PartialEq)]
#[setters(into)]
pub struct RetryConfig {
    /// Initial backoff delay in milliseconds for retry operations
    #[merge(strategy = crate::merge::std::overwrite)]
    pub initial_backoff_ms: u64,

    /// Backoff multiplication factor for each retry attempt
    #[merge(strategy = crate::merge::std::overwrite)]
    pub backoff_factor: u64,

    /// Maximum number of retry attempts
    #[merge(strategy = crate::merge::std::overwrite)]
    pub max_retry_attempts: usize,

    /// HTTP status codes that should trigger retries (e.g., 429, 500, 502, 503,
    /// 504)
    #[merge(strategy = crate::merge::std::overwrite)]
    pub retry_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_backoff_ms: 200,
            backoff_factor: 2,
            max_retry_attempts: 8,
            retry_status_codes: vec![429, 500, 502, 503, 504],
        }
    }
}

impl RetryConfig {
    /// Retry wrapper for operations that may fail with retryable errors
    pub async fn retry<T, FutureFn, Fut>(&self, operation: FutureFn) -> anyhow::Result<T>
    where
        FutureFn: FnMut() -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let strategy = ExponentialBuilder::default()
            .with_min_delay(Duration::from_millis(0))
            .with_factor(self.backoff_factor as f32)
            .with_max_times(self.max_retry_attempts)
            .with_jitter();

        operation
            .retry(strategy)
            .when(should_retry)
            .await
            .with_context(|| "Failed to execute operation with retry")
    }
}

/// Determines if an error should trigger a retry attempt.
///
/// This function checks if the error is a retryable domain error.
/// Currently, only `Error::Retryable` errors will trigger retries.
fn should_retry(error: &anyhow::Error) -> bool {
    let retry = error
        .downcast_ref::<Error>()
        .is_some_and(|error| matches!(error, Error::Retryable(_, _)));

    warn!(error = %error, retry = retry, "Retrying on error");
    retry
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_retry_success_on_first_attempt() {
        // Fixture: Create retry config and successful operation
        let retry_config = RetryConfig::default();
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        // Actual: Execute operation that succeeds immediately
        let actual = retry_config
            .retry(|| {
                let mut count = call_count_clone.lock().unwrap();
                *count += 1;
                async move { Ok::<i32, anyhow::Error>(42) }
            })
            .await;

        // Expected: Should succeed on first try
        assert!(actual.is_ok());
        assert_eq!(actual.unwrap(), 42);
        assert_eq!(*call_count.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_retry_with_retryable_error() {
        use crate::Error;

        // Fixture: Create retry config and operation that fails then succeeds
        let total_count = 5usize;
        let retry_config = RetryConfig::default()
            .max_retry_attempts(total_count)
            .initial_backoff_ms(0u64)
            .backoff_factor(1u64);
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        // Actual: Execute operation that fails once then succeeds
        let actual: anyhow::Result<()> = retry_config
            .retry(|| async {
                let mut count = call_count_clone.lock().unwrap();
                *count += 1;
                Err(anyhow::anyhow!(Error::Retryable(
                    1,
                    anyhow::anyhow!("Test retryable error")
                )))
            })
            .await;

        // Expected: Should succeed after retry
        assert!(actual.is_err());
        assert_eq!(*call_count.lock().unwrap(), total_count + 1);
    }
}
