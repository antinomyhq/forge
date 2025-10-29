use serde::{Deserialize, Serialize};

/// Session identifier
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    PartialEq,
    Eq,
    Debug,
)]
#[serde(transparent)]
pub struct SessionId(String);

/// API key name/label
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct ApiKeyName(String);

/// URL parameter for API key authentication
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    Hash,
    derive_more::From,
    derive_more::Display,
)]
#[serde(transparent)]
pub struct URLParam(String);

/// URL parameter value
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_more::Deref, derive_more::From,
)]
#[serde(transparent)]
pub struct URLParamValue(String);
