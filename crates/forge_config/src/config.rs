use serde::{Deserialize, Serialize};

/// Root configuration type for the forge_config crate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub templates: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<UpdateConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_failure_per_turn: Option<usize>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests_per_turn: Option<usize>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact: Option<CompactConfig>,
}

/// Configuration for automatic update checks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// How often to check for updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<UpdateFrequency>,

    /// Whether to automatically apply updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
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

/// Configuration for automatic context compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactConfig {
    /// Number of most recent messages to preserve during compaction.
    #[serde(default)]
    pub retention_window: usize,

    /// Maximum percentage of the context that can be summarized during compaction.
    #[serde(default, deserialize_with = "deserialize_percentage")]
    pub eviction_window: f64,

    /// Maximum number of tokens to keep after compaction.
    pub max_tokens: Option<usize>,

    /// Maximum number of tokens before triggering compaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_threshold: Option<usize>,

    /// Maximum number of conversation turns before triggering compaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_threshold: Option<usize>,

    /// Maximum number of messages before triggering compaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_threshold: Option<usize>,

    /// Optional tag name to extract content from when summarizing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_tag: Option<SummaryTag>,

    /// Whether to trigger compaction when the last message is from a user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_turn_end: Option<bool>,
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
