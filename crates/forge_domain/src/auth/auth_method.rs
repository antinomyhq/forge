use serde::{Deserialize, Serialize};

use super::OAuthConfig;

/// Authentication method configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    ApiKey,
    #[serde(rename = "oauth_device")]
    OAuthDevice(OAuthConfig),
    #[serde(rename = "oauth_code")]
    OAuthCode(OAuthConfig),
    #[serde(rename = "google_adc")]
    GoogleAdc,
    #[serde(rename = "codex_device")]
    CodexDevice(OAuthConfig),
}

impl AuthMethod {
    pub fn oauth_device(config: OAuthConfig) -> Self {
        Self::OAuthDevice(config)
    }

    pub fn oauth_code(config: OAuthConfig) -> Self {
        Self::OAuthCode(config)
    }

    pub fn google_adc() -> Self {
        Self::GoogleAdc
    }

    /// Creates a Codex device auth method
    pub fn codex_device(config: OAuthConfig) -> Self {
        Self::CodexDevice(config)
    }

    pub fn oauth_config(&self) -> Option<&OAuthConfig> {
        match self {
            Self::OAuthDevice(config) | Self::OAuthCode(config) | Self::CodexDevice(config) => {
                Some(config)
            }
            Self::ApiKey | Self::GoogleAdc => None,
        }
    }
}
