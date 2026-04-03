use derive_setters::Setters;
use fake::Dummy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Decimal, ReasoningConfig};

/// A named collection of LLM inference settings that can be referenced by id
/// from a model configuration.
#[derive(Default, Debug, Setters, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Dummy)]
#[serde(rename_all = "snake_case")]
pub struct Preset {
    /// Output randomness for the model; lower values are deterministic, higher
    /// values are creative (0.0–2.0).
    #[serde(default)]
    pub temperature: Decimal,

    /// Nucleus sampling threshold; limits token selection to the top cumulative
    /// probability mass (0.0–1.0).
    #[serde(default)]
    pub top_p: Decimal,

    /// Top-k vocabulary cutoff; restricts sampling to the k
    /// highest-probability tokens (1–1000).
    #[serde(default)]
    pub top_k: u32,

    /// Maximum tokens the model may generate per response (1–100,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Reasoning configuration; controls effort level, token budget, and
    /// visibility of the model's thinking process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,

    /// Whether tool use is supported; when false, all tool calls are disabled.
    #[serde(default)]
    pub tool_supported: bool,
}
