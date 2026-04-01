use std::path::PathBuf;

use derive_setters::Setters;
use fake::Dummy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::reader::ConfigReader;
use crate::writer::ConfigWriter;
use crate::{
    AutoDumpFormat, Compact, Decimal, HttpConfig, ModelConfig, ReasoningConfig, RetryConfig,
    Update,
};

/// Top-level Forge configuration merged from all sources (defaults, file,
/// environment).
#[derive(Default, Debug, Setters, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Dummy)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option)]
pub struct ForgeConfig {
    /// Retry settings applied at the system level to all IO operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
    /// Maximum number of lines returned by a single file search operation.
    #[serde(default)]
    pub max_search_lines: usize,
    /// Maximum number of bytes returned by a single file search operation.
    #[serde(default)]
    pub max_search_result_bytes: usize,
    /// Maximum number of characters returned from a URL fetch.
    #[serde(default)]
    pub max_fetch_chars: usize,
    /// Maximum number of lines captured from the leading portion of shell
    /// command output.
    #[serde(default)]
    pub max_stdout_prefix_lines: usize,
    /// Maximum number of lines captured from the trailing portion of shell
    /// command output.
    #[serde(default)]
    pub max_stdout_suffix_lines: usize,
    /// Maximum number of characters per line in shell command output.
    #[serde(default)]
    pub max_stdout_line_chars: usize,
    /// Maximum number of characters per line when reading a file.
    #[serde(default)]
    pub max_line_chars: usize,
    /// Maximum number of lines read from a file in a single operation.
    #[serde(default)]
    pub max_read_lines: u64,
    /// Maximum number of files read in a single batch operation.
    #[serde(default)]
    pub max_file_read_batch_size: usize,
    /// HTTP client settings including proxy, TLS, and timeout configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpConfig>,
    /// Maximum file size in bytes permitted for read operations.
    #[serde(default)]
    pub max_file_size_bytes: u64,
    /// Maximum image file size in bytes permitted for read operations.
    #[serde(default)]
    pub max_image_size_bytes: u64,
    /// Maximum time in seconds a single tool call may run before being
    /// cancelled.
    #[serde(default)]
    pub tool_timeout_secs: u64,
    /// Whether to automatically open HTML dump files in the browser after
    /// creation.
    #[serde(default)]
    pub auto_open_dump: bool,
    /// Directory where debug request files are written; disabled when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_requests: Option<PathBuf>,
    /// Path to the conversation history file; defaults to the global history
    /// location when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_history_path: Option<PathBuf>,
    /// Maximum number of conversations shown in the conversation list.
    #[serde(default)]
    pub max_conversations: usize,
    /// Maximum number of candidate results returned from the initial semantic
    /// search vector query.
    #[serde(default)]
    pub max_sem_search_results: usize,
    /// Number of top results retained after re-ranking in semantic search.
    #[serde(default)]
    pub sem_search_top_k: usize,
    /// Base URL of the Forge services API used for semantic search and
    /// indexing.
    #[serde(default)]
    #[dummy(expr = "\"https://api.forgecode.dev/api\".to_string()")]
    pub services_url: String,
    /// Maximum number of file extensions included in the agent system prompt.
    #[serde(default)]
    pub max_extensions: usize,
    /// Format used when automatically creating a session dump after task
    /// completion; disabled when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_dump: Option<AutoDumpFormat>,
    /// Maximum number of files read concurrently during batch operations.
    #[serde(default)]
    pub max_parallel_file_reads: usize,
    /// Time-to-live in seconds for the cached model API list.
    #[serde(default)]
    pub model_cache_ttl_secs: u64,
    /// Default model and provider configuration used when not overridden by
    /// individual agents.    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<ModelConfig>,
    /// Model and provider configuration used for commit message generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<ModelConfig>,
    /// Model and provider configuration used for shell command suggestion
    /// generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest: Option<ModelConfig>,

    // --- Workflow fields ---
    /// Configuration for automatic Forge updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

    /// Whether restricted mode is active; when enabled, tool execution requires
    /// explicit permission grants.
    #[serde(default)]
    pub restricted: bool,

    /// Whether tool use is supported in the current environment; when false,
    /// all tool calls are disabled.
    #[serde(default)]
    pub tool_supported: bool,

    /// Reasoning configuration applied to all agents; controls effort level,
    /// token budget, and visibility of the model's thinking process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
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
            .read_legacy()
            .read_defaults()
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
