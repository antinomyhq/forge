use derive_more::derive::Display;
use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ProviderId;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Setters)]
pub struct Model {
    pub id: ModelId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<ProviderId>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub tools_supported: Option<bool>,
    /// Whether the model supports parallel tool calls
    pub supports_parallel_tool_calls: Option<bool>,
    /// Whether the model supports reasoning
    pub supports_reasoning: Option<bool>,
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
