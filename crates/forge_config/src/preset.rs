use derive_setters::Setters;
use fake::Dummy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Effort level for reasoning; controls the depth of model thinking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Dummy)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// Minimal reasoning; fastest and cheapest.
    Low,
    /// Balanced reasoning effort.
    Medium,
    /// Maximum reasoning depth; slowest and most expensive.
    High,
    /// Beyond maximum reasoning depth; highest cost and latency.
    XHigh,
}

/// Reasoning configuration for a preset.
/// Controls how and whether models engage extended chain-of-thought reasoning.
#[derive(Debug, Setters, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Dummy)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option, into)]
pub struct ReasoningConfig {
    /// Effort level for reasoning; controls the depth of model thinking.
    /// Supported by OpenRouter and the Forge provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    /// Maximum number of tokens the model may spend on reasoning.
    /// Supported by OpenRouter, Anthropic, and the Forge provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    /// When true, the model reasons internally but reasoning output is hidden.
    /// Supported by OpenRouter and the Forge provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,

    /// Enables reasoning at the "medium" effort level with no exclusions.
    /// Supported by OpenRouter, Anthropic, and the Forge provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// A named collection of LLM-specific sampling and generation parameters.
/// Presets are referenced by their `id` from model configurations and agent
/// definitions to apply a consistent set of inference settings.
#[derive(Debug, Setters, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Dummy)]
#[serde(rename_all = "snake_case")]
#[setters(strip_option, into)]
pub struct PresetConfig {
    /// Unique identifier for this preset. Required for catalog entries in
    /// [`ForgeConfig::presets`]; not meaningful for inline uses such as
    /// [`ForgeConfig::session_preset`].
    pub id: String,

    /// Output randomness; lower values are deterministic, higher values are
    /// creative (0.0–2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Nucleus sampling threshold; limits token selection to the top
    /// cumulative probability mass (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Top-k vocabulary cutoff; restricts sampling to the k
    /// highest-probability tokens (1–1000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,

    /// Maximum tokens the model may generate per response (1–100,000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Reasoning configuration; controls extended chain-of-thought thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}
