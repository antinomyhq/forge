use std::collections::HashMap;
use std::path::PathBuf;

use derive_setters::Setters;
use merge::Merge;
use serde::Deserialize;
use url::Url;

use crate::{
    AutoDumpFormat, HttpConfig, ModelConfig, ModelId, ProviderId, RetryConfig,
    reader::ConfigReader, writer::ConfigWriter,
};

/// Forge configuration containing all the fields from the Environment struct.
///
/// # Field Naming Convention
///
/// Fields follow these rules to make units and semantics unambiguous at the
/// call-site:
///
/// - **Unit suffixes are mandatory** for any numeric field that carries a
///   physical unit:
///   - `_ms`    — duration in milliseconds
///   - `_secs`  — duration in seconds
///   - `_bytes` — size in bytes
///   - `_lines` — count of text lines
///   - `_chars` — count of characters
///   - Pure counts / dimensionless values (e.g. `max_redirects`) carry no
///     suffix.
///
/// - **`max_` is always a prefix**, never embedded mid-name:
///   - Correct:   `max_stdout_prefix_lines`
///   - Incorrect: `stdout_max_prefix_length`
///
/// - **No redundant struct-name prefixes inside a sub-struct**: fields inside
///   `RetryConfig` must not repeat `retry_` (e.g. use `status_codes`, not
///   `retry_status_codes`).
///
/// - **`_limit` is avoided**; prefer the explicit `max_` prefix + unit suffix
///   instead.
#[derive(Debug, Setters, Clone, PartialEq, Deserialize, fake::Dummy, Merge)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option)]
pub struct ForgeConfig {
    /// Configuration for the retry mechanism
    pub retry: RetryConfig,
    /// The maximum number of lines returned for FSSearch
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_search_lines: usize,
    /// Maximum bytes allowed for search results
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_search_result_bytes: usize,
    /// Maximum characters for fetch content
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_fetch_chars: usize,
    /// Maximum lines for shell output prefix
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_stdout_prefix_lines: usize,
    /// Maximum lines for shell output suffix
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_stdout_suffix_lines: usize,
    /// Maximum characters per line for shell output
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_stdout_line_chars: usize,
    /// Maximum characters per line for file read operations
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_line_chars: usize,
    /// Maximum number of lines to read from a file
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_read_lines: u64,
    /// Maximum number of files that can be read in a single batch operation
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_file_read_batch_size: usize,
    /// HTTP configuration
    pub http: HttpConfig,
    /// Maximum file size in bytes for operations
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_file_size_bytes: u64,
    /// Maximum image file size in bytes for binary read operations
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_image_size_bytes: u64,
    /// Maximum execution time in seconds for a single tool call
    #[merge(strategy = merge::num::overwrite_zero)]
    pub tool_timeout_secs: u64,
    /// Whether to automatically open HTML dump files in the browser
    #[merge(strategy = merge::bool::overwrite_false)]
    pub auto_open_dump: bool,
    /// Path where debug request files should be written
    #[merge(strategy = merge::option::overwrite_none)]
    pub debug_requests: Option<PathBuf>,
    /// Custom history file path
    #[merge(strategy = merge::option::overwrite_none)]
    pub custom_history_path: Option<PathBuf>,
    /// Maximum number of conversations to show in list
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_conversations: usize,
    /// Maximum number of results to return from initial vector search
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_sem_search_results: usize,
    /// Top-k parameter for relevance filtering during semantic search
    #[merge(strategy = merge::num::overwrite_zero)]
    pub sem_search_top_k: usize,
    /// URL for the indexing server
    #[dummy(expr = "url::Url::parse(\"http://localhost:8080\").unwrap()")]
    #[merge(strategy = crate::merge::overwrite)]
    pub workspace_server_url: Url,
    /// Maximum number of file extensions to include in the system prompt
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_extensions: usize,
    /// Format for automatically creating a dump when a task is completed
    #[merge(strategy = merge::option::overwrite_none)]
    pub auto_dump: Option<AutoDumpFormat>,
    /// Maximum number of files read concurrently in parallel operations
    #[merge(strategy = merge::num::overwrite_zero)]
    pub max_parallel_file_reads: usize,
    /// TTL in seconds for the model API list cache
    #[merge(strategy = merge::num::overwrite_zero)]
    pub model_cache_ttl_secs: u64,
    /// Default provider ID to use for AI operations
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub provider: Option<ProviderId>,
    /// Map of provider ID to model ID for per-provider model selection
    #[serde(default)]
    #[merge(strategy = merge::hashmap::overwrite)]
    pub model: HashMap<ProviderId, ModelId>,
    /// Provider and model to use for commit message generation
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub commit: Option<ModelConfig>,
    /// Provider and model to use for shell command suggestion generation
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub suggest: Option<ModelConfig>,
    /// API key for Forge authentication
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub api_key: Option<String>,
    /// Display name of the API key
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub api_key_name: Option<String>,
    /// Masked representation of the API key for display purposes
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub api_key_masked: Option<String>,
    /// Email address associated with the Forge account
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub email: Option<String>,
    /// Display name of the authenticated user
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub name: Option<String>,
    /// Identifier of the authentication provider used for login
    #[serde(default)]
    #[merge(strategy = merge::option::overwrite_none)]
    pub auth_provider_id: Option<String>,
}

impl ForgeConfig {
    /// Get the global ForgeConfig instance, loading from the embedded config
    /// file on first access.
    ///
    /// # Panics
    ///
    /// Panics if the configuration cannot be loaded.
    pub async fn read() -> ForgeConfig {
        ConfigReader::new()
            .read()
            .await
            .expect("Failed to load configuration")
    }

    /// Writes the configuration to the user config file.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be serialized or written to
    /// disk.
    pub async fn write(&self) -> crate::Result<()> {
        ConfigWriter::new(self.clone()).write().await
    }
}
