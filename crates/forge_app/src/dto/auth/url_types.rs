use serde::{Deserialize, Serialize};

/// OAuth authorization endpoint URL
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
pub struct AuthUrl(String);

/// OAuth token endpoint URL
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
pub struct TokenUrl(String);

/// Verification URI for device flow
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
pub struct VerificationUri(String);

/// Authorization URL for code flow
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
pub struct AuthorizationUrl(String);
