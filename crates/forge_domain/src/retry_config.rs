use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

// Maximum number of retry attempts for retryable operations
const MAX_RETRY_ATTEMPTS: usize = 3;

const RETRY_STATUS_CODES: &[u16] = &[429, 500, 502, 503, 504];

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

    /// Maximum delay between retries in seconds
    #[merge(strategy = crate::merge::std::overwrite)]
    pub max_delay: Option<u64>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_backoff_ms: 200,
            backoff_factor: 2,
            max_retry_attempts: MAX_RETRY_ATTEMPTS,
            retry_status_codes: RETRY_STATUS_CODES.to_vec(),
            max_delay: None,
        }
    }
}
