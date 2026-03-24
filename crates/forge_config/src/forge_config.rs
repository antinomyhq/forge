use std::path::PathBuf;

use derive_setters::Setters;
use serde::Deserialize;
use url::Url;

use crate::{AutoDumpFormat, HttpConfig, RetryConfig};

/// Forge configuration containing all the fields from the Environment struct.
///
/// # Field Naming Convention
///
/// Fields follow these rules to make units and semantics unambiguous at the call-site:
///
/// - **Unit suffixes are mandatory** for any numeric field that carries a physical unit:
///   - `_ms`    — duration in milliseconds
///   - `_secs`  — duration in seconds
///   - `_bytes` — size in bytes
///   - `_lines` — count of text lines
///   - `_chars` — count of characters
///   - Pure counts / dimensionless values (e.g. `max_redirects`) carry no suffix.
///
/// - **`max_` is always a prefix**, never embedded mid-name:
///   - Correct:   `max_stdout_prefix_lines`
///   - Incorrect: `stdout_max_prefix_length`
///
/// - **No redundant struct-name prefixes inside a sub-struct**: fields inside `RetryConfig`
///   must not repeat `retry_` (e.g. use `status_codes`, not `retry_status_codes`).
///
/// - **`_limit` is avoided**; prefer the explicit `max_` prefix + unit suffix instead.
#[derive(Debug, Setters, Clone, PartialEq, Deserialize, fake::Dummy)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option)]
pub struct ForgeConfig {
    /// The shell being used
    pub shell: String,
    /// Base URL for Forge's backend APIs
    #[dummy(expr = "url::Url::parse(\"https://example.com\").unwrap()")]
    pub forge_api_url: Url,
    /// Configuration for the retry mechanism
    pub retry_config: RetryConfig,
    /// The maximum number of lines returned for FSSearch
    pub max_search_lines: usize,
    /// Maximum bytes allowed for search results
    pub max_search_result_bytes: usize,
    /// Maximum characters for fetch content
    pub max_fetch_chars: usize,
    /// Maximum lines for shell output prefix
    pub max_stdout_prefix_lines: usize,
    /// Maximum lines for shell output suffix
    pub max_stdout_suffix_lines: usize,
    /// Maximum characters per line for shell output
    pub max_stdout_line_chars: usize,
    /// Maximum characters per line for file read operations
    pub max_line_chars: usize,
    /// Maximum number of lines to read from a file
    pub max_read_lines: u64,
    /// Maximum number of files that can be read in a single batch operation
    pub max_file_read_batch_size: usize,
    /// HTTP configuration
    pub http: HttpConfig,
    /// Maximum file size in bytes for operations
    pub max_file_size_bytes: u64,
    /// Maximum image file size in bytes for binary read operations
    pub max_image_size_bytes: u64,
    /// Maximum execution time in seconds for a single tool call
    pub tool_timeout_secs: u64,
    /// Whether to automatically open HTML dump files in the browser
    pub auto_open_dump: bool,
    /// Path where debug request files should be written
    pub debug_requests: Option<PathBuf>,
    /// Custom history file path
    pub custom_history_path: Option<PathBuf>,
    /// Maximum number of conversations to show in list
    pub max_conversations: usize,
    /// Maximum number of results to return from initial vector search
    pub max_sem_search_results: usize,
    /// Top-k parameter for relevance filtering during semantic search
    pub sem_search_top_k: usize,
    /// URL for the indexing server
    #[dummy(expr = "url::Url::parse(\"http://localhost:8080\").unwrap()")]
    pub workspace_server_url: Url,
    /// Maximum number of file extensions to include in the system prompt
    pub max_extensions: usize,
    /// Format for automatically creating a dump when a task is completed
    pub auto_dump: Option<AutoDumpFormat>,
    /// Maximum number of files read concurrently in parallel operations
    pub parallel_file_reads: usize,
    /// TTL in seconds for the model API list cache
    pub model_cache_ttl_secs: u64,
}

impl ForgeConfig {
    /// Load configuration from the embedded config file using the config crate.
    pub fn get() -> Result<Self, config::ConfigError> {
        let config = config::Config::builder()
            .add_source(config::File::from_str(
                include_str!("../.config.json"),
                config::FileFormat::Json,
            ))
            .build()?;

        config.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::TlsBackend;

    #[test]
    fn test_forge_config_fields() {
        let config = ForgeConfig {
            shell: "zsh".to_string(),
            forge_api_url: "https://api.example.com".parse().unwrap(),
            retry_config: RetryConfig {
                initial_backoff_ms: 200,
                min_delay_ms: 1000,
                backoff_factor: 2,
                max_retry_attempts: 8,
                status_codes: vec![429, 500, 502, 503, 504],
                max_delay: None,
                suppress_retry_errors: false,
            },
            max_search_lines: 1000,
            max_search_result_bytes: 10240,
            max_fetch_chars: 50000,
            max_stdout_prefix_lines: 100,
            max_stdout_suffix_lines: 100,
            max_stdout_line_chars: 500,
            max_line_chars: 2000,
            max_read_lines: 2000,
            max_file_read_batch_size: 50,
            http: HttpConfig {
                connect_timeout_secs: 30,
                read_timeout_secs: 900,
                pool_idle_timeout_secs: 90,
                pool_max_idle_per_host: 5,
                max_redirects: 10,
                hickory: false,
                tls_backend: TlsBackend::Default,
                min_tls_version: None,
                max_tls_version: None,
                adaptive_window: true,
                keep_alive_interval_secs: Some(60),
                keep_alive_timeout_secs: 10,
                keep_alive_while_idle: true,
                accept_invalid_certs: false,
                root_cert_paths: None,
            },
            max_file_size_bytes: 104857600,
            tool_timeout_secs: 300,
            auto_open_dump: false,
            debug_requests: None,
            custom_history_path: None,
            max_conversations: 100,
            max_sem_search_results: 100,
            sem_search_top_k: 10,
            max_image_size_bytes: 262144,
            workspace_server_url: "http://localhost:8080".parse().unwrap(),
            max_extensions: 15,
            auto_dump: None,
            parallel_file_reads: 64,
            model_cache_ttl_secs: 604_800,
        };

        assert_eq!(config.shell, "zsh");
        assert_eq!(config.max_search_lines, 1000);
    }

    #[test]
    fn test_forge_config_get() {
        let config = ForgeConfig::get().expect("Failed to load config");
        assert_eq!(config.shell, "bash");
        assert_eq!(config.tool_timeout_secs, 300);
    }
}
