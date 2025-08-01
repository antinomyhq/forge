use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{Error, RetryConfig};

pub async fn retry_with_config<F, Fut, T, C>(
    config: &RetryConfig,
    operation: F,
    notify: Option<C>,
) -> anyhow::Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
    C: Fn(&anyhow::Error, Duration) + Send + Sync + 'static,
{
    let strategy = ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(config.min_delay_ms))
        .with_factor(config.backoff_factor as f32)
        .with_max_times(config.max_retry_attempts)
        .with_jitter();

    let retryable = operation.retry(&strategy).when(should_retry);

    match notify {
        Some(callback) => retryable.notify(callback).await,
        None => retryable.await,
    }
}

/// Retry with exhaustion event sending.
///
/// This function will retry the operation up to the configured maximum attempts.
/// If all configured retries are exhausted and the operation still fails, it will
/// send a RetryExhausted event through the provided sender.
pub async fn retry_with_exhaustion_event<F, Fut, T, C>(
    config: &RetryConfig,
    operation: F,
    notify: Option<C>,
    exhaustion_sender: Option<impl Fn(&anyhow::Error, usize) + Send + Sync + 'static>,
) -> anyhow::Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
    C: Fn(&anyhow::Error, Duration) + Send + Sync + 'static,
{
    let strategy = ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(config.min_delay_ms))
        .with_factor(config.backoff_factor as f32)
        .with_max_times(config.max_retry_attempts)
        .with_jitter();

    let retryable = operation.retry(&strategy).when(should_retry);

    match notify {
        Some(callback) => {
            match retryable.notify(callback).await {
                Ok(result) => Ok(result),
                Err(error) => {
                    // If the error is retryable, send exhaustion event
                    if should_retry(&error) {
                        if let Some(sender) = exhaustion_sender {
                            sender(&error, config.max_retry_attempts);
                        }
                    }
                    Err(error)
                }
            }
        }
        None => {
            match retryable.await {
                Ok(result) => Ok(result),
                Err(error) => {
                    // If the error is retryable, send exhaustion event
                    if should_retry(&error) {
                        if let Some(sender) = exhaustion_sender {
                            sender(&error, config.max_retry_attempts);
                        }
                    }
                    Err(error)
                }
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_retry_with_exhaustion_event() {
        let config = RetryConfig::default().max_retry_attempts(2usize);
        let exhaustion_called = Arc::new(Mutex::new(false));
        let exhaustion_called_clone = exhaustion_called.clone();

        // Create an operation that always fails with a retryable error
        let operation = || async {
            Err::<(), anyhow::Error>(anyhow::Error::from(Error::Retryable(anyhow::anyhow!(
                "Network error"
            ))))
        };

        // Create an exhaustion sender that marks when it's called
        let exhaustion_sender = move |_: &anyhow::Error, _: usize| {
            let exhaustion_called = exhaustion_called_clone.clone();
            tokio::spawn(async move {
                let mut called = exhaustion_called.lock().await;
                *called = true;
            });
        };

        // Run the retry function
        let result: Result<(), anyhow::Error> = retry_with_exhaustion_event(
            &config,
            operation,
            None::<fn(&anyhow::Error, Duration)>,
            Some(exhaustion_sender),
        )
        .await;

        // Verify that the operation failed
        assert!(result.is_err());

        // Give a moment for the async task to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Verify that the exhaustion sender was called
        let called = exhaustion_called.lock().await;
        assert!(*called);
    }
}
