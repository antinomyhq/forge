use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Root structure containing all providers
pub type ModelsDevRegistry = HashMap<String, Provider>;

/// A provider (e.g., OpenAI, Anthropic, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider {
    /// Unique identifier for the provider
    pub id: String,

    /// Environment variables required for authentication
    #[serde(default)]
    pub env: Vec<String>,

    /// NPM package name for SDK integration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,

    /// API base URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,

    /// Human-readable provider name
    pub name: String,

    /// URL to documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,

    /// Map of model ID to model details
    pub models: HashMap<String, Model>,
}

/// Individual model information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// Unique identifier for the model
    pub id: String,

    /// Human-readable model name
    pub name: String,

    /// Whether the model supports attachments
    pub attachment: bool,

    /// Whether the model has reasoning capabilities
    pub reasoning: bool,

    /// Whether the model supports temperature parameter
    pub temperature: bool,

    /// Whether the model supports function/tool calling
    pub tool_call: bool,

    /// Knowledge cutoff date (e.g., "2024-10")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<String>,

    /// Model release date (YYYY-MM-DD)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,

    /// Last update date (YYYY-MM-DD)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,

    /// Input and output modalities
    pub modalities: Modalities,

    /// Whether model weights are open
    pub open_weights: bool,

    /// Pricing information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,

    /// Context and output limits
    pub limit: Limit,
}

/// Input and output modalities supported by the model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modalities {
    /// Input modalities (e.g., ["text", "image"])
    pub input: Vec<String>,

    /// Output modalities (e.g., ["text"])
    pub output: Vec<String>,
}

/// Pricing information for the model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    /// Cost per million input tokens
    pub input: f64,

    /// Cost per million output tokens
    pub output: f64,

    /// Optional cost for reading from cache
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,

    /// Optional cost for writing to cache
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,

    /// Optional cost for reasoning tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<f64>,
}

/// Context and output token limits
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    /// Maximum context length in tokens
    pub context: u64,

    /// Maximum output length in tokens
    pub output: u64,
}
