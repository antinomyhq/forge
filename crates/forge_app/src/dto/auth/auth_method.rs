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
}

impl AuthMethod {
    pub fn oauth_device(config: OAuthConfig) -> Self {
        Self::OAuthDevice(config)
    }

    pub fn oauth_code(config: OAuthConfig) -> Self {
        Self::OAuthCode(config)
    }

    pub fn oauth_config(&self) -> Option<&OAuthConfig> {
        match self {
            Self::OAuthDevice(config) | Self::OAuthCode(config) => Some(config),
            Self::ApiKey => None,
        }
    }

    /// Converts AuthMethod to AuthType
    pub fn to_auth_type(&self) -> crate::dto::AuthType {
        match self {
            Self::ApiKey => crate::dto::AuthType::ApiKey,
            Self::OAuthDevice(config) | Self::OAuthCode(config) => {
                if config.token_refresh_url.is_some() {
                    crate::dto::AuthType::OAuthWithApiKey
                } else {
                    crate::dto::AuthType::OAuth
                }
            }
        }
    }
}
