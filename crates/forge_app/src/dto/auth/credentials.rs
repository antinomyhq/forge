use serde::{Deserialize, Serialize};

/// API key for authentication
///
/// API key credential
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Hash, Debug,
)]
#[serde(transparent)]
pub struct ApiKey(String);

/// OAuth access token
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
pub struct AccessToken(String);

/// OAuth refresh token
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct RefreshToken(String);

/// Device code for OAuth device flow
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct DeviceCode(String);

/// User code for OAuth device flow
///
/// This is displayed to the user, so it has a standard Debug implementation.
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
pub struct UserCode(String);

/// State parameter for CSRF protection in OAuth flows
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct State(String);

/// OAuth client identifier
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
pub struct ClientId(String);

/// PKCE code verifier
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct PkceVerifier(String);

/// Authorization code from OAuth code flow
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct AuthorizationCode(String);
