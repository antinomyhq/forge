use derive_more::{AsRef, Deref, Display, From};
use merge::Merge;
use serde::{Deserialize, Serialize};

/// A newtype wrapper for a provider identifier string.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Display,
    From,
    AsRef,
    Deref,
    fake::Dummy,
)]
pub struct ProviderId(String);

impl ProviderId {
    /// Creates a new `ProviderId` from the given string value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// A newtype wrapper for a model identifier string.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Display,
    From,
    AsRef,
    Deref,
    fake::Dummy,
)]
pub struct ModelId(String);

impl ModelId {
    /// Creates a new `ModelId` from the given string value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Pairs a provider and model together for a specific operation.
#[derive(Debug, Clone, PartialEq, Deserialize, fake::Dummy, Merge)]
pub struct ModelConfig {
    /// The provider to use for this operation.
    #[serde(rename = "provider")]
    #[merge(strategy = crate::merge::overwrite)]
    pub provider_id: ProviderId,
    /// The model to use for this operation.
    #[serde(rename = "model")]
    #[merge(strategy = crate::merge::overwrite)]
    pub model_id: ModelId,
}
