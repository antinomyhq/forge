use serde::{Deserialize, Serialize};
use strum_macros::Display;

/// Type of authentication used for a provider
#[derive(Debug, Clone, PartialEq, Display, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum AuthType {
    /// Traditional API key authentication
    ApiKey,

    /// OAuth authentication (device or code flow)
    #[serde(rename = "oauth")]
    #[strum(serialize = "oauth")]
    OAuth,

    /// OAuth token used to fetch an API key (GitHub Copilot pattern)
    #[serde(rename = "oauth_with_api_key")]
    #[strum(serialize = "oauth_with_api_key")]
    OAuthWithApiKey,
}
