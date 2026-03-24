use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ModelId, ProviderId};

/// Per-agent model and provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Setters, JsonSchema, PartialEq)]
#[setters(into)]
pub struct AgentModelConfig {
    /// Provider ID to use for this agent.
    pub provider: ProviderId,
    /// Model ID to use for this agent.
    pub model: ModelId,
}
