use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::read::read;

/// Root configuration type for the forge_config crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ForgeConfig {
    /// Format for automatically creating a dump when a task is completed.
    /// Set to "json" (or "true"/"1"/"yes") for JSON, "html" for HTML, or
    /// omit to disable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_dump: Option<AutoDumpFormat>,

    /// Whether to automatically open HTML dump files in the browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_open_dump: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact: Option<CompactConfig>,

    /// Model configuration for commit message generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<ModelConfig>,

    /// Custom history file path. If omitted, uses the default history path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_history_path: Option<String>,

    /// Path where debug request files should be written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_requests: Option<String>,

    /// Maximum characters for fetch content truncation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetch_truncation_limit: Option<usize>,

    /// Base URL for Forge's backend APIs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge_api_url: Option<String>,

    /// HTTP client configuration (timeouts, TLS, connection pooling).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpConfig>,

    /// Maximum number of conversations to show in list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_conversations: Option<usize>,

    /// Maximum number of file extensions to include in the system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_extensions: Option<usize>,

    /// Maximum number of files that can be read in a single batch operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_file_read_batch_size: Option<usize>,

    /// Maximum file size in bytes for operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_file_size: Option<u64>,

    /// Maximum image file size in bytes for binary read operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_image_size: Option<u64>,

    /// Maximum characters per line for file read operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_line_length: Option<usize>,

    /// Maximum number of lines to read from a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_read_size: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests_per_turn: Option<usize>,

    /// Maximum number of lines returned for FSSearch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_search_lines: Option<usize>,

    /// Maximum bytes allowed for search results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_search_result_bytes: Option<usize>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_failure_per_turn: Option<usize>,

    /// Default model configuration used when no task-specific model is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelConfig>,

    /// Maximum number of files read concurrently in parallel operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_file_reads: Option<usize>,

    /// Configuration for the retry mechanism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_config: Option<RetryConfig>,

    /// Maximum number of results to return from initial vector search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sem_search_limit: Option<usize>,

    /// Top-k parameter for relevance filtering during semantic search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sem_search_top_k: Option<usize>,

    /// Maximum characters per line for shell output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_max_line_length: Option<usize>,

    /// Maximum lines for shell output prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_max_prefix_length: Option<usize>,

    /// Maximum lines for shell output suffix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_max_suffix_length: Option<usize>,

    /// Model configuration for code suggestion generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest: Option<ModelConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub templates: Option<String>,

    /// Maximum execution time in seconds for a single tool call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_timeout: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<UpdateConfig>,

    /// URL for the workspace indexing server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_server_url: Option<String>,
}

impl ForgeConfig {
    /// Reads a [`ForgeConfig`] from YAML, JSON, and environment variable sources.
    ///
    /// # Arguments
    ///
    /// * `path` - Base file path without extension. `.yaml` and `.json` variants are probed
    ///   automatically.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] if any source fails to parse or deserialization fails.
    pub async fn read(path: &str) -> Result<Self, Error> {
        read(path).await
    }
}

/// Configuration for automatic update checks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Whether to automatically apply updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,

    /// How often to check for updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<UpdateFrequency>,
}

/// Frequency at which update checks are performed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFrequency {
    Daily,
    Weekly,
    #[default]
    Always,
}

/// Model selection configuration specifying a provider and model identifier.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Identifier of the model to use (e.g. `"gpt-4o"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    /// Identifier of the provider that hosts the model (e.g. `"openai"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

/// The output format used when auto-dumping a conversation on task completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoDumpFormat {
    /// Dump as a JSON file.
    Json,
    /// Dump as an HTML file.
    Html,
}

/// Configuration for automatic context compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactConfig {
    /// Maximum percentage of the context that can be summarized during compaction.
    #[serde(default, deserialize_with = "deserialize_percentage")]
    pub eviction_window: f64,

    /// Maximum number of tokens to keep after compaction.
    pub max_tokens: Option<usize>,

    /// Maximum number of messages before triggering compaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_threshold: Option<usize>,

    /// Whether to trigger compaction when the last message is from a user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_turn_end: Option<bool>,

    /// Number of most recent messages to preserve during compaction.
    #[serde(default)]
    pub retention_window: usize,

    /// Optional tag name to extract content from when summarizing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_tag: Option<SummaryTag>,

    /// Maximum number of tokens before triggering compaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_threshold: Option<usize>,

    /// Maximum number of conversation turns before triggering compaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_threshold: Option<usize>,
}

fn deserialize_percentage<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let value = f64::deserialize(deserializer)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(Error::custom(format!(
            "percentage must be between 0.0 and 1.0, got {value}"
        )));
    }

    Ok(value)
}

/// Tag name to extract content from during summarization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SummaryTag(pub String);

/// Configuration for the HTTP retry mechanism.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Initial backoff delay in milliseconds for retry operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_factor: Option<u64>,

    /// Initial backoff delay in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_backoff_ms: Option<u64>,

    /// Maximum delay between retries in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_delay: Option<u64>,

    /// Maximum number of retry attempts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retry_attempts: Option<usize>,

    /// Minimum delay in milliseconds between retry attempts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_delay_ms: Option<u64>,

    /// HTTP status codes that should trigger retries (e.g., 429, 500, 502).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_status_codes: Option<Vec<u16>>,

    /// Whether to suppress retry error logging and events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppress_retry_errors: Option<bool>,
}

/// HTTP client configuration (timeouts, connection pooling, TLS, HTTP/2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Accept invalid TLS certificates. Use with caution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accept_invalid_certs: Option<bool>,

    /// Enable HTTP/2 adaptive window sizing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_window: Option<bool>,

    /// Connection timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout: Option<u64>,

    /// Use Hickory DNS resolver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hickory: Option<bool>,

    /// Keep-alive interval in seconds. Set to `null` to disable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_interval: Option<u64>,

    /// Keep-alive timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_timeout: Option<u64>,

    /// Keep-alive while connection is idle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive_while_idle: Option<bool>,

    /// Maximum number of redirects to follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<usize>,

    /// Maximum TLS protocol version to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tls_version: Option<TlsVersion>,

    /// Minimum TLS protocol version to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_tls_version: Option<TlsVersion>,

    /// Maximum idle connections per host in the connection pool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_idle_timeout: Option<u64>,

    /// Pool idle timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_max_idle_per_host: Option<usize>,

    /// Read timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_timeout: Option<u64>,

    /// Paths to root certificate files (PEM, CRT, CER format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_cert_paths: Option<Vec<String>>,

    /// TLS backend to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_backend: Option<TlsBackend>,
}

/// TLS backend selection for HTTP connections.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlsBackend {
    #[default]
    Default,
    Rustls,
}

/// TLS protocol version constraint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum TlsVersion {
    #[serde(rename = "1.0")]
    V1_0,
    #[serde(rename = "1.1")]
    V1_1,
    #[serde(rename = "1.2")]
    V1_2,
    #[default]
    #[serde(rename = "1.3")]
    V1_3,
}
