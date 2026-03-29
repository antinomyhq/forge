use std::path::PathBuf;

use derive_setters::Setters;
use fake::Dummy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::reader::ConfigReader;
use crate::writer::ConfigWriter;
use crate::{AutoDumpFormat, Compact, Decimal, HttpConfig, ModelConfig, RetryConfig, Update};

/// Top-level Forge configuration merged from all sources (defaults, file,
/// environment).
#[derive(Default, Debug, Setters, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Dummy)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option)]
pub struct ForgeConfig {
    /// Configuration for the retry mechanism
    pub retry: Option<RetryConfig>,
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
    pub http: Option<HttpConfig>,
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
    #[dummy(expr = "\"https://example.com/api\".to_string()")]
    pub services_url: String,
    /// Maximum number of file extensions to include in the system prompt
    pub max_extensions: usize,
    /// Format for automatically creating a dump when a task is completed
    pub auto_dump: Option<AutoDumpFormat>,
    /// Maximum number of files read concurrently in parallel operations
    pub max_parallel_file_reads: usize,
    /// TTL in seconds for the model API list cache
    pub model_cache_ttl_secs: u64,
    /// Default model and provider configuration used when not overridden by
    /// individual agents.    
    #[serde(default)]
    pub session: Option<ModelConfig>,
    /// Provider and model to use for commit message generation    
    #[serde(default)]
    pub commit: Option<ModelConfig>,
    /// Provider and model to use for shell command suggestion generation    
    #[serde(default)]
    pub suggest: Option<ModelConfig>,

    // --- Workflow fields ---
    /// Configuration for automatic forge updates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updates: Option<Update>,

    /// Output randomness for all agents; lower values are deterministic, higher
    /// values are creative (0.0–2.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Decimal>,

    /// Nucleus sampling threshold for all agents; limits token selection to the
    /// top cumulative probability mass (0.0–1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<Decimal>,

    /// Top-k vocabulary cutoff for all agents; restricts sampling to the k
    /// highest-probability tokens (1–1000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    /// Maximum tokens the model may generate per response for all agents
    /// (1–100,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Maximum tool failures per turn before the orchestrator forces
    /// completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests_per_turn: Option<usize>,

    /// Context compaction settings applied to all agents; falls back to each
    /// agent's individual setting when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact: Option<Compact>,

    /// Whether the application is running in restricted mode.
    /// When true, tool execution requires explicit permission grants.    
    pub restricted: bool,

    /// Whether tool use is supported in the current environment.
    /// When false, tool calls are disabled regardless of agent configuration.
    pub tool_supported: bool,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::reader::ConfigReader;

    #[test]
    fn test_f32_temperature_round_trip() {
        let fixture = ForgeConfig { temperature: Some(Decimal(0.1)), ..Default::default() };

        let toml = toml_edit::ser::to_string_pretty(&fixture).unwrap();

        assert!(
            toml.contains("temperature = 0.1\n"),
            "expected `temperature = 0.1` in TOML output, got:\n{toml}"
        );
    }

    #[test]
    fn test_f32_top_p_round_trip() {
        let fixture = ForgeConfig { top_p: Some(Decimal(0.9)), ..Default::default() };

        let toml = toml_edit::ser::to_string_pretty(&fixture).unwrap();

        assert!(
            toml.contains("top_p = 0.9\n"),
            "expected `top_p = 0.9` in TOML output, got:\n{toml}"
        );
    }

    #[test]
    fn test_f32_temperature_deserialize_round_trip() {
        let fixture = ForgeConfig { temperature: Some(Decimal(0.1)), ..Default::default() };

        let toml = toml_edit::ser::to_string_pretty(&fixture).unwrap();

        let actual = ConfigReader::default().read_toml(&toml).build().unwrap();

        assert_eq!(actual.temperature, fixture.temperature);
    }
}

impl ForgeConfig {
    /// Reads and merges configuration from all sources, returning the resolved
    /// [`ForgeConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error if the config path cannot be resolved, the file cannot
    /// be read, or deserialization fails.
    pub fn read() -> crate::Result<ForgeConfig> {
        ConfigReader::default()
            .read_defaults()
            .read_legacy()
            .read_global()
            .read_env()
            .build()
    }

    /// Writes the configuration to the user config file.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be serialized or written to
    /// disk.
    pub fn write(&self) -> crate::Result<()> {
        let path = ConfigReader::config_path();
        ConfigWriter::new(self.clone()).write(&path)
    }
}
