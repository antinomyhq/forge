use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ModelId, ProviderId};

/// Configuration for commit message generation.
///
/// Allows specifying a dedicated provider and model for commit message
/// generation, instead of using the active agent's provider and model. This is
/// useful when you want to use a cheaper or faster model for simple commit
/// message generation.
#[derive(Debug, Clone, Serialize, Deserialize, Setters, JsonSchema, PartialEq)]
#[setters(into)]
pub struct CommitConfig {
    /// Provider ID to use for commit message generation.
    pub provider: ProviderId,

    /// Model ID to use for commit message generation.
    pub model: ModelId,
}

impl CommitConfig {
    /// Creates a new [`CommitConfig`] with the given provider and model.
    pub fn new(provider: impl Into<ProviderId>, model: impl Into<ModelId>) -> Self {
        Self { provider: provider.into(), model: model.into() }
    }
}
