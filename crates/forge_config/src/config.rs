use serde::{Deserialize, Serialize};

/// Root configuration type for the forge_config crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
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

    /// Custom history file path. If omitted, uses the default history path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_history_path: Option<String>,

    /// Path where debug request files should be written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_requests: Option<String>,

    /// Maximum characters for fetch content truncation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetch_truncation_limit: Option<usize>,

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

    /// Maximum number of files read concurrently in parallel operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_file_reads: Option<usize>,

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
