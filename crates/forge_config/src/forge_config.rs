use std::path::PathBuf;

use derive_setters::Setters;
use serde::Deserialize;
use url::Url;

use crate::{AutoDumpFormat, HttpConfig, RetryConfig, TlsBackend};

/// Forge configuration containing all the fields from the Environment struct.
#[derive(Debug, Setters, Clone, PartialEq, Deserialize, fake::Dummy)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
pub struct ForgeConfig {
    /// The operating system of the environment
    pub os: String,
    /// The process ID of the current process
    pub pid: u32,
    /// The current working directory
    pub cwd: PathBuf,
    /// The home directory
    pub home: Option<PathBuf>,
    /// The shell being used
    pub shell: String,
    /// The base path relative to which everything else stored
    pub base_path: PathBuf,
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
    pub fetch_truncation_limit: usize,
    /// Maximum lines for shell output prefix
    pub stdout_max_prefix_length: usize,
    /// Maximum lines for shell output suffix
    pub stdout_max_suffix_length: usize,
    /// Maximum characters per line for shell output
    pub stdout_max_line_length: usize,
    /// Maximum characters per line for file read operations
    pub max_line_length: usize,
    /// Maximum number of lines to read from a file
    pub max_read_size: u64,
    /// Maximum number of files that can be read in a single batch operation
    pub max_file_read_batch_size: usize,
    /// HTTP configuration
    pub http: HttpConfig,
    /// Maximum file size in bytes for operations
    pub max_file_size: u64,
    /// Maximum image file size in bytes for binary read operations
    pub max_image_size: u64,
    /// Maximum execution time in seconds for a single tool call
    pub tool_timeout: u64,
    /// Whether to automatically open HTML dump files in the browser
    pub auto_open_dump: bool,
    /// Path where debug request files should be written
    pub debug_requests: Option<PathBuf>,
    /// Custom history file path
    pub custom_history_path: Option<PathBuf>,
    /// Maximum number of conversations to show in list
    pub max_conversations: usize,
    /// Maximum number of results to return from initial vector search
    pub sem_search_limit: usize,
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
    pub model_cache_ttl: u64,
}

impl ForgeConfig {
    /// Load configuration from the embedded config file using the config crate.
    pub fn get() -> Result<Self, config::ConfigError> {
        let config = config::Config::builder()
            .add_source(config::File::from_string(
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
            os: "linux".to_string(),
            pid: 1234,
            cwd: PathBuf::from("/current/working/dir"),
            home: Some(PathBuf::from("/home/user")),
            shell: "zsh".to_string(),
            base_path: PathBuf::from("/home/user/.forge"),
            forge_api_url: "https://api.example.com".parse().unwrap(),
            retry_config: RetryConfig {
                initial_backoff_ms: 200,
                min_delay_ms: 1000,
                backoff_factor: 2,
                max_retry_attempts: 8,
                retry_status_codes: vec![429, 500, 502, 503, 504],
                max_delay: None,
                suppress_retry_errors: false,
            },
            max_search_lines: 1000,
            max_search_result_bytes: 10240,
            fetch_truncation_limit: 50000,
            stdout_max_prefix_length: 100,
            stdout_max_suffix_length: 100,
            stdout_max_line_length: 500,
            max_line_length: 2000,
            max_read_size: 2000,
            max_file_read_batch_size: 50,
            http: HttpConfig {
                connect_timeout: 30,
                read_timeout: 900,
                pool_idle_timeout: 90,
                pool_max_idle_per_host: 5,
                max_redirects: 10,
                hickory: false,
                tls_backend: TlsBackend::Default,
                min_tls_version: None,
                max_tls_version: None,
                adaptive_window: true,
                keep_alive_interval: Some(60),
                keep_alive_timeout: 10,
                keep_alive_while_idle: true,
                accept_invalid_certs: false,
                root_cert_paths: None,
            },
            max_file_size: 104857600,
            tool_timeout: 300,
            auto_open_dump: false,
            debug_requests: None,
            custom_history_path: None,
            max_conversations: 100,
            sem_search_limit: 100,
            sem_search_top_k: 10,
            max_image_size: 262144,
            workspace_server_url: "http://localhost:8080".parse().unwrap(),
            max_extensions: 15,
            auto_dump: None,
            parallel_file_reads: 64,
            model_cache_ttl: 604_800,
        };

        assert_eq!(config.os, "linux");
        assert_eq!(config.pid, 1234);
        assert_eq!(config.max_search_lines, 1000);
    }

    #[test]
    fn test_forge_config_get() {
        let config = ForgeConfig::get().expect("Failed to load config");
        assert_eq!(config.os, "linux");
        assert_eq!(config.tool_timeout, 300);
    }
}
