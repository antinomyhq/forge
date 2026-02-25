use derive_setters::Setters;
use merge::Merge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ModelId, ProviderId};

/// Configuration for commit message generation.
///
/// Allows specifying a dedicated provider and model for commit message generation,
/// instead of using the active agent's provider and model. This is useful when
/// you want to use a cheaper or faster model for simple commit message generation.
#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters, JsonSchema, PartialEq)]
#[setters(strip_option, into)]
pub struct CommitConfig {
    /// Provider ID to use for commit message generation.
    /// If not specified, the active agent's provider will be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub provider: Option<ProviderId>,

    /// Model ID to use for commit message generation.
    /// If not specified, the provider's default model or the active agent's
    /// model will be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub model: Option<ModelId>,
}
