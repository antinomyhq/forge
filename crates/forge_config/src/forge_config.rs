use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use derive_setters::Setters;
use dirs;
use serde::Deserialize;
use url::Url;

use crate::{AutoDumpFormat, HttpConfig, ModelConfig, ModelId, ProviderId, RetryConfig};

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
#[derive(Debug, Setters, Clone, PartialEq, Deserialize, fake::Dummy)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option)]
pub struct ForgeConfig {
    /// Configuration for the retry mechanism
    pub retry: RetryConfig,
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
    pub max_parallel_file_reads: usize,
    /// TTL in seconds for the model API list cache
    pub model_cache_ttl_secs: u64,
    /// Default provider ID to use for AI operations
    #[serde(default)]
    pub provider: Option<ProviderId>,
    /// Map of provider ID to model ID for per-provider model selection
    #[serde(default)]
    pub model: HashMap<ProviderId, ModelId>,
    /// Provider and model to use for commit message generation
    #[serde(default)]
    pub commit: Option<ModelConfig>,
    /// Provider and model to use for shell command suggestion generation
    #[serde(default)]
    pub suggest: Option<ModelConfig>,
    /// API key for Forge authentication
    #[serde(default)]
    pub api_key: Option<String>,
    /// Display name of the API key
    #[serde(default)]
    pub api_key_name: Option<String>,
    /// Masked representation of the API key for display purposes
    #[serde(default)]
    pub api_key_masked: Option<String>,
    /// Email address associated with the Forge account
    #[serde(default)]
    pub email: Option<String>,
    /// Display name of the authenticated user
    #[serde(default)]
    pub name: Option<String>,
    /// Identifier of the authentication provider used for login
    #[serde(default)]
    pub auth_provider_id: Option<String>,
}

static CONFIG: OnceLock<ForgeConfig> = OnceLock::new();

impl ForgeConfig {
    /// Get the global ForgeConfig instance, loading from the embedded config
    /// file on first access.
    ///
    /// # Panics
    ///
    /// Panics if the configuration cannot be loaded.
    pub fn get() -> &'static ForgeConfig {
        CONFIG.get_or_init(|| {
            let mut builder = config::Config::builder()
                .add_source(config::File::from_str(
                    include_str!("../.config.json"),
                    config::FileFormat::Json,
                ));

            // Add user config from home directory if it exists
            if let Some(config_dir) = dirs::home_dir() {
                let user_config_path = config_dir.join("forge").join(".config.json");
                if user_config_path.exists() {
                    builder = builder.add_source(config::File::from(user_config_path));
                }
            }

            let config = builder
                .build()
                .expect("Failed to build config");

            config
                .try_deserialize()
                .expect("Failed to deserialize config")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forge_config_get() {
        let _ = ForgeConfig::get();
    }
}
