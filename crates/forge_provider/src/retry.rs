use std::time::Duration;

use reqwest_eventsource::retry::RetryPolicy;
use reqwest_eventsource::Error;

/// A RetryPolicy that only retries on specific status codes
pub struct StatusCodeRetryPolicy {
    /// The inner backoff policy
    inner: reqwest_eventsource::retry::ExponentialBackoff,
    /// Status codes that should trigger a retry
    retry_status_codes: Vec<u16>,
}

impl StatusCodeRetryPolicy {
    /// Create a new status code specific retry policy
    pub fn new(
        start: Duration,
        factor: f64,
        max_duration: Option<Duration>,
        max_retries: Option<usize>,
        retry_status_codes: Vec<u16>,
    ) -> Self {
        Self {
            inner: reqwest_eventsource::retry::ExponentialBackoff::new(
                start,
                factor,
                max_duration,
                max_retries,
            ),
            retry_status_codes,
        }
    }
}

impl RetryPolicy for StatusCodeRetryPolicy {
    fn retry(&self, error: &Error, last_retry: Option<(usize, Duration)>) -> Option<Duration> {
        // Only retry for specific status codes
        match error {
            Error::InvalidStatusCode(status_code, _) => {
                // Check if the status code is in our retry list
                if self.retry_status_codes.contains(&status_code.as_u16()) {
                    // Delegate to inner policy for backoff calculation
                    self.inner.retry(error, last_retry)
                } else {
                    // Don't retry for status codes not in our list
                    None
                }
            }
            // For network/transport errors, always retry
            Error::Transport(_) => self.inner.retry(error, last_retry),
            // Don't retry for other types of errors
            _ => None,
        }
    }

    fn set_reconnection_time(&mut self, duration: Duration) {
        self.inner.set_reconnection_time(duration)
    }
}
