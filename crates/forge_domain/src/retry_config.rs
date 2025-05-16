use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};

// Maximum number of retry attempts for retryable operations
// Increased from 3 to 5 to better handle transient network issues
const MAX_RETRY_ATTEMPTS: usize = 5;

// Status codes that should trigger retries:
// 408 - Request Timeout
// 429 - Too Many Requests
// 500 - Internal Server Error
// 502 - Bad Gateway
// 503 - Service Unavailable
// 504 - Gateway Timeout
// 507 - Insufficient Storage
// 509 - Bandwidth Limit Exceeded
// 520-526 - Cloudflare errors
const RETRY_STATUS_CODES: &[u16] = &[
    408, 425, 429, 500, 502, 503, 504, 507, 509, 520, 521, 522, 523, 524, 525, 526,
];

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
            initial_backoff_ms: 500,
            backoff_factor: 2,
            max_retry_attempts: MAX_RETRY_ATTEMPTS,
            retry_status_codes: RETRY_STATUS_CODES.to_vec(),
        }
    }
}
