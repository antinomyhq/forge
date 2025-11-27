use derive_more::derive::Display;
use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Setters)]
pub struct Model {
    pub id: ModelId,
    pub name: Option<String>,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    // TODO: add provider information to the model
    pub tools_supported: Option<bool>,
    /// Whether the model supports parallel tool calls
    pub supports_parallel_tool_calls: Option<bool>,
    /// Whether the model supports reasoning
    pub supports_reasoning: Option<bool>,
    /// Pricing information for the model (per token costs in USD)
    pub pricing: Option<Pricing>,
}

/// Pricing information for a model (per token costs in USD)
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Pricing {
    /// Cost per prompt token (input)
    pub prompt: Option<f32>,
    /// Cost per completion token (output)
    pub completion: Option<f32>,
    /// Cost per image
    pub image: Option<f32>,
    /// Cost per request
    pub request: Option<f32>,
    /// Cost to write tokens to cache (Anthropic-specific)
    pub cache_write: Option<f32>,
    /// Cost to read tokens from cache (Anthropic-specific)
    pub cache_read: Option<f32>,
}

impl Pricing {
    /// Calculate cost for Anthropic-style usage with cache support
    ///
    /// # Arguments
    ///
    /// * `input_tokens` - Standard input tokens (not from cache)
    /// * `output_tokens` - Output/completion tokens
    /// * `cache_creation_tokens` - Tokens written to cache
    /// * `cache_read_tokens` - Tokens read from cache
    ///
    /// # Errors
    ///
    /// Returns None if required pricing information is missing
    pub fn calculate_anthropic_cost(
        &self,
        input_tokens: usize,
        output_tokens: usize,
        cache_creation_tokens: usize,
        cache_read_tokens: usize,
    ) -> Option<f64> {
        let prompt_cost = self.prompt? as f64 * input_tokens as f64;
        let completion_cost = self.completion? as f64 * output_tokens as f64;
        let cache_write_cost = self.cache_write.unwrap_or(0.0) as f64 * cache_creation_tokens as f64;
        let cache_read_cost = self.cache_read.unwrap_or(0.0) as f64 * cache_read_tokens as f64;

        Some(prompt_cost + completion_cost + cache_write_cost + cache_read_cost)
    }
}


#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Parameters {
    pub tool_supported: bool,
}

impl Parameters {
    pub fn new(tool_supported: bool) -> Self {
        Self { tool_supported }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Hash, Eq, Display, JsonSchema)]
#[serde(transparent)]
pub struct ModelId(String);

impl ModelId {
    pub fn new<T: Into<String>>(id: T) -> Self {
        Self(id.into())
    }
}

impl From<String> for ModelId {
    fn from(value: String) -> Self {
        ModelId(value)
    }
}

impl From<&str> for ModelId {
    fn from(value: &str) -> Self {
        ModelId(value.to_string())
    }
}

impl ModelId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
