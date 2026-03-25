use serde::{Deserialize, Serialize};

/// A type alias for a provider identifier string.
pub type ProviderId = String;

/// A type alias for a model identifier string.
pub type ModelId = String;

/// Pairs a provider and model together for a specific operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, fake::Dummy)]
pub struct ModelConfig {
    /// The provider to use for this operation.
    pub provider_id: String,
    /// The model to use for this operation.
    pub model_id: String,
}
