/// Generic provider authentication flow types
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// OAuth config for device/code flows
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub use_pkce: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_refresh_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_auth_params: Option<std::collections::HashMap<String, String>>,
}

/// Authentication method type
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
    /// Create OAuth device method
    pub fn oauth_device(config: OAuthConfig) -> Self {
        Self::OAuthDevice(config)
    }

    /// Create OAuth code method
    pub fn oauth_code(config: OAuthConfig) -> Self {
        Self::OAuthCode(config)
    }

    /// Returns a reference to the OAuth config if this is an OAuth method
    pub fn oauth_config(&self) -> Option<&OAuthConfig> {
        match self {
            Self::OAuthDevice(config) | Self::OAuthCode(config) => Some(config),
            Self::ApiKey => None,
        }
    }
}

/// Result of initiating authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthInitiation {
    ApiKeyPrompt {
        required_params: Vec<URLParam>,
    },

    DeviceFlow {
        user_code: String,
        verification_uri: String,
        verification_uri_complete: Option<String>,
        expires_in: u64,
        interval: u64,
        context: AuthContext,
    },

    CodeFlow {
        authorization_url: String,
        state: String,
        context: AuthContext,
    },
}

/// Type-safe auth context storage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthContext {
    ApiKey {
        api_key: String,
        url_params: std::collections::HashMap<String, String>,
    },
    Device {
        device_code: String,
        interval: u64,
    },
    Code {
        state: String,
        pkce_verifier: Option<String>,
    },
}

impl Default for AuthContext {
    fn default() -> Self {
        Self::ApiKey {
            api_key: String::new(),
            url_params: std::collections::HashMap::new(),
        }
    }
}

impl AuthContext {
    /// Create device context
    pub fn device(device_code: String, interval: u64) -> Self {
        Self::Device { device_code, interval }
    }

    /// Create code context
    pub fn code(state: String, pkce_verifier: Option<String>) -> Self {
        Self::Code { state, pkce_verifier }
    }
}

/// Auth result data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResult {
    ApiKey {
        api_key: String,
        url_params: HashMap<String, String>,
    },

    OAuthTokens {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
    },

    AuthorizationCode {
        code: String,
        state: String,
        code_verifier: Option<String>,
    },
}

/// URL parameter key for providers requiring additional configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_more::Deref)]
#[serde(transparent)]
pub struct URLParam(String);

impl AsRef<str> for URLParam {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
