use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ModelId, ProviderId};

/// Configuration for shell command suggestion generation.
///
/// Allows specifying a dedicated provider and model for shell command
/// suggestion generation, instead of using the active agent's provider and
/// model. This is useful when you want to use a cheaper or faster model for
/// simple command suggestions. Both provider and model must be specified
/// together.
#[derive(Debug, Clone, Serialize, Deserialize, Setters, JsonSchema, PartialEq)]
#[setters(into)]
pub struct SuggestConfig {
    /// Provider ID to use for command suggestion generation.
    pub provider: ProviderId,

    /// Model ID to use for command suggestion generation.
    pub model: ModelId,
}
