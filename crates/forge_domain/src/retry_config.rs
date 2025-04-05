use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

// Maximum number of retry attempts for retryable operations
pub const MAX_RETRY_ATTEMPTS: usize = 3;

pub const RETRY_STATUS_CODES: &[u16] = &[429, 500, 502, 503, 504];

#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters, PartialEq)]
#[setters(strip_option, into)]
pub struct RetryConfig {
    /// Initial backoff delay in milliseconds for retry operations
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_backoff_ms: Option<u64>,

    /// Backoff multiplication factor for each retry attempt
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff_factor: Option<u64>,

    /// Maximum number of retry attempts
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retry_attempts: Option<usize>,

    /// HTTP status codes that should trigger retries (e.g., 429, 500, 502, 503,
    /// 504)
    #[merge(strategy = crate::merge::option)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_status_codes: Option<Vec<u16>>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_backoff_ms: Some(200),
            backoff_factor: Some(2),
            max_retry_attempts: Some(MAX_RETRY_ATTEMPTS),
            retry_status_codes: Some(RETRY_STATUS_CODES.to_vec()),
        }
    }
}
