use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A type alias for a provider identifier string.
pub type ProviderId = String;

/// A type alias for a model identifier string.
pub type ModelId = String;

/// Pairs a provider and model together for a specific operation.
#[derive(
    Default, Debug, Setters, Clone, PartialEq, Serialize, Deserialize, JsonSchema, fake::Dummy,
)]
#[setters(strip_option, into)]
pub struct ModelConfig {
    /// The provider to use for this operation.
    pub provider_id: Option<String>,
    /// The model to use for this operation.
    pub model_id: Option<String>,
}
