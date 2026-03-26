use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{CommitConfig, ModelId, ProviderId, SuggestConfig};

/// Domain-level session configuration pairing a provider with a model.
///
/// Used inside [`Environment`] to represent the active session, decoupled from
/// the on-disk configuration format.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct SessionConfig {
    /// The active provider ID (e.g. `"anthropic"`).
    pub provider_id: Option<String>,
    /// The model ID to use with this provider.
    pub model_id: Option<String>,
}

/// All discrete mutations that can be applied to the application configuration.
///
/// Instead of replacing the entire config, callers describe exactly which field
/// they want to change. Implementations receive a list of operations, apply
/// each in order, and persist the result atomically.
#[derive(Debug, Clone, PartialEq)]
pub enum AppConfigOperation {
    /// Set the active provider.
    SetProvider(ProviderId),
    /// Set the model for the given provider.
    SetModel(ProviderId, ModelId),
    /// Set the commit-message generation configuration.
    SetCommitConfig(CommitConfig),
    /// Set the shell-command suggestion configuration.
    SetSuggestConfig(SuggestConfig),
}
