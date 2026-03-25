use std::path::PathBuf;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::reader::ConfigReader;
use crate::writer::ConfigWriter;
use crate::{
    AutoDumpFormat, Compact, HttpConfig, MaxTokens, ModelConfig, RetryConfig, Temperature, TopK,
    TopP, Update,
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
#[derive(Debug, Setters, Clone, PartialEq, Serialize, Deserialize, fake::Dummy)]
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
    /// Default provider_id to use for all models if not specified
    #[serde(default)]
    pub provider: Option<String>,
    /// Map of provider ID to model ID for per-provider model selection
    #[serde(default)]
    pub model: Option<ModelConfig>,
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

    // --- Workflow fields ---
    /// Configuration for automatic forge updates
    #[serde(skip_serializing_if = "Option::is_none")]
    #[dummy(default)]
    pub updates: Option<Update>,

    /// Temperature used for all agents.
    ///
    /// Temperature controls the randomness in the model's output.
    /// - Lower values (e.g., 0.1) make responses more focused, deterministic,
    ///   and coherent
    /// - Higher values (e.g., 0.8) make responses more creative, diverse, and
    ///   exploratory
    /// - Valid range is 0.0 to 2.0
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[dummy(expr = "Some(Temperature::new(1.0).unwrap())")]
    pub temperature: Option<Temperature>,

    /// Top-p (nucleus sampling) used for all agents.
    ///
    /// Controls the diversity of the model's output by considering only the
    /// most probable tokens up to a cumulative probability threshold.
    /// - Lower values (e.g., 0.1) make responses more focused
    /// - Higher values (e.g., 0.9) make responses more diverse
    /// - Valid range is 0.0 to 1.0
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[dummy(expr = "Some(TopP::new(0.9).unwrap())")]
    pub top_p: Option<TopP>,

    /// Top-k used for all agents.
    ///
    /// Controls the number of highest probability vocabulary tokens to keep.
    /// - Lower values (e.g., 10) make responses more focused
    /// - Higher values (e.g., 100) make responses more diverse
    /// - Valid range is 1 to 1000
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[dummy(expr = "Some(TopK::new(50).unwrap())")]
    pub top_k: Option<TopK>,

    /// Maximum number of tokens the model can generate for all agents.
    ///
    /// Controls the maximum length of the model's response.
    /// - Lower values (e.g., 100) limit response length for concise outputs
    /// - Higher values (e.g., 4000) allow for longer, more detailed responses
    /// - Valid range is 1 to 100,000
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[dummy(expr = "Some(MaxTokens::new(4000).unwrap())")]
    pub max_tokens: Option<MaxTokens>,

    /// Maximum number of times a tool can fail before the orchestrator
    /// forces the completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests_per_turn: Option<usize>,

    /// Configuration for automatic context compaction for all agents.
    /// If specified, this will be applied to all agents in the workflow.
    /// If not specified, each agent's individual setting will be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[dummy(default)]
    pub compact: Option<Compact>,
}

impl ForgeConfig {
    /// Returns the path to the user configuration file: `~/.forge/.forge.toml`.
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined.
    pub fn config_path() -> crate::Result<PathBuf> {
        let home_dir = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "home directory not found")
        })?;
        Ok(home_dir.join("forge").join(".forge.toml"))
    }

    /// Reads and merges configuration from all sources, returning the resolved
    /// [`ForgeConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error if the config path cannot be resolved, the file cannot
    /// be read, or the configuration cannot be deserialized.
    pub async fn read() -> crate::Result<ForgeConfig> {
        let path = Self::config_path()?;
        ConfigReader::new().read(Some(&path)).await
    }

    /// Writes the configuration to the user config file.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be serialized or written to
    /// disk.
    pub async fn write(&self) -> crate::Result<()> {
        let path = Self::config_path()?;
        ConfigWriter::new(self.clone()).write(&path).await
    }
}
