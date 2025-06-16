use std::future::Future;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{Error, RetryConfig};

/// Retry wrapper for operations that may fail with retryable errors
pub async fn retry_with_config<T, FutureFn, Fut>(
    config: &RetryConfig,
    operation: FutureFn,
) -> anyhow::Result<T>
where
    FutureFn: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let strategy = ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(config.min_delay_ms))
        .with_factor(config.backoff_factor as f32)
        .with_max_times(config.max_retry_attempts)
        .with_jitter();

    operation.retry(strategy).when(should_retry).await
}

/// Determines if an error should trigger a retry attempt.
///
/// This function checks if the error is a retryable domain error.
/// Currently, only `Error::Retryable` errors will trigger retries.
fn should_retry(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Error>()
        .is_some_and(|error| matches!(error, Error::Retryable(_)))
}
