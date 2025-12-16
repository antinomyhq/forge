use derive_setters::Setters;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Setters, PartialEq, fake::Dummy)]
#[serde(rename_all = "snake_case")]
#[setters(into)]
pub struct RetryConfig {
    /// Initial backoff delay in milliseconds for retry operations
    pub initial_backoff_ms: u64,

    /// Minimum delay in milliseconds between retry attempts
    pub min_delay_ms: u64,

    /// Backoff multiplication factor for each retry attempt
    pub backoff_factor: u64,

    /// Maximum number of retry attempts
    pub max_retry_attempts: usize,

    /// HTTP status codes that should trigger retries (e.g., 429, 500, 502, 503,
    /// 504)
    pub retry_status_codes: Vec<u16>,

    /// Maximum delay between retries in seconds
    pub max_delay: Option<u64>,

    /// Whether to suppress retry error logging and events
    pub suppress_retry_errors: bool,
}


