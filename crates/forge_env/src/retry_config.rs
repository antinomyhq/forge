use derive_setters::Setters;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Setters, PartialEq, fake::Dummy)]
#[serde(rename_all = "camelCase")]
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

impl RetryConfig {
    /// Creates a RetryConfig with default values from env.json.
    ///
    /// This is primarily intended for testing but can be used anywhere defaults
    /// from the JSON configuration are needed.
    pub fn test_default() -> Self {
        const DEFAULT_CONFIG: &str = include_str!("../env.json");
        let env: serde_json::Value =
            serde_json::from_str(DEFAULT_CONFIG).expect("Failed to parse env.json");
        serde_json::from_value(env["retryConfig"].clone())
            .expect("Failed to deserialize retryConfig from env.json")
    }
}

impl RetryConfig {
    // Implementation moved to forge_app::retry module to avoid backon dependency
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_retry_config_default() {
        // Test that retry config can be deserialized from JSON with expected values
        let json = r#"{
            "initialBackoffMs": 200,
            "minDelayMs": 1000,
            "backoffFactor": 2,
            "maxRetryAttempts": 8,
            "retryStatusCodes": [429, 500, 502, 503, 504, 408],
            "maxDelay": null,
            "suppressRetryErrors": false
        }"#;

        let config: RetryConfig = serde_json::from_str(json).unwrap();

        // Expected: Should have expected values
        assert_eq!(config.initial_backoff_ms, 200);
        assert_eq!(config.min_delay_ms, 1000);
        assert_eq!(config.backoff_factor, 2);
        assert_eq!(config.max_retry_attempts, 8);
        assert_eq!(
            config.retry_status_codes,
            vec![429, 500, 502, 503, 504, 408]
        );
        assert_eq!(config.suppress_retry_errors, false);
    }

    #[test]
    fn test_retry_config_setters() {
        // Fixture: Create retry config with custom values
        let config = RetryConfig::test_default()
            .initial_backoff_ms(100u64)
            .min_delay_ms(500u64)
            .backoff_factor(3u64)
            .max_retry_attempts(5usize)
            .retry_status_codes(vec![429, 503])
            .suppress_retry_errors(true);

        // Expected: Should have custom values
        assert_eq!(config.initial_backoff_ms, 100);
        assert_eq!(config.min_delay_ms, 500);
        assert_eq!(config.backoff_factor, 3);
        assert_eq!(config.max_retry_attempts, 5);
        assert_eq!(config.retry_status_codes, vec![429, 503]);
        assert_eq!(config.suppress_retry_errors, true);
    }
}
