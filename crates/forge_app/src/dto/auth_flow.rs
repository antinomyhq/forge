/// Authentication flow types for generic provider authentication.
///
/// This module provides types that support all provider authentication
/// patterns:
/// - Simple API key authentication
/// - OAuth Device Flow
/// - OAuth + API Key Exchange (GitHub Copilot pattern)
/// - OAuth Authorization Code Flow
/// - Cloud Service Account with Parameters (Vertex AI, Azure)
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// OAuth configuration for device and code flows
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// Device code URL (device flow only)
    /// Example: "https://github.com/login/device/code"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code_url: Option<String>,

    /// Device token URL (device flow only)
    /// Example: "https://github.com/login/oauth/access_token"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_token_url: Option<String>,

    /// Authorization URL (code flow only)
    /// Example: "https://claude.ai/oauth/authorize"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,

    /// Token exchange URL (code flow only)
    /// Example: "https://api.anthropic.com/oauth/token"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,

    /// OAuth client ID provided by the service
    pub client_id: String,

    /// List of OAuth scopes to request
    pub scopes: Vec<String>,

    /// Redirect URI (for code flow, points to provider's callback page)
    /// Example: "https://console.anthropic.com/oauth/code/callback"
    pub redirect_uri: String,

    /// Whether to use PKCE (Proof Key for Code Exchange) for security
    #[serde(default)]
    pub use_pkce: bool,

    /// URL to fetch API key from OAuth token (GitHub Copilot pattern)
    /// Example: "https://api.github.com/copilot_internal/v2/token"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_refresh_url: Option<String>,

    /// Custom HTTP headers for OAuth requests (provider-specific)
    /// Allows providers like GitHub to specify required headers (e.g.,
    /// User-Agent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_headers: Option<std::collections::HashMap<String, String>>,

    /// Extra query parameters to add to authorization URL (provider-specific)
    /// Example: For Claude.ai, add {"code": "true"}
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_auth_params: Option<std::collections::HashMap<String, String>>,
}

/// Authentication method type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Direct API key entry
    ApiKey,
    /// OAuth device flow (display code to user)
    #[serde(rename = "oauth_device")]
    OAuthDevice(OAuthConfig),
    /// OAuth authorization code flow (redirect to browser)
    #[serde(rename = "oauth_code")]
    OAuthCode(OAuthConfig),
}

impl AuthMethod {
    /// Creates a new OAuth device flow authentication method
    pub fn oauth_device(config: OAuthConfig) -> Self {
        Self::OAuthDevice(config)
    }

    /// Creates a new OAuth code flow authentication method
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
    /// API key auth - prompt user for key and required parameters.
    ///
    /// For simple providers (OpenAI, Anthropic), `required_params` is empty.
    /// For cloud providers (Vertex AI, Azure), includes parameters like
    /// project_id, location, etc.
    /// All parameters listed here are required by default.
    ApiKeyPrompt {
        /// Required parameter keys for cloud providers (project_id, location,
        /// etc.) Empty for simple API key providers (OpenAI, Anthropic)
        required_params: Vec<URLParam>,
    },

    /// Device flow - display code and URL to user.
    ///
    /// User should visit `verification_uri` and enter `user_code`.
    /// Optionally, they can visit `verification_uri_complete` to skip manual
    /// code entry.
    DeviceFlow {
        /// Code user should enter at the verification URL
        user_code: String,
        /// URL where user should authenticate
        verification_uri: String,
        /// Optional URL that includes the code (user can skip manual entry)
        verification_uri_complete: Option<String>,
        /// How long the code is valid (seconds)
        expires_in: u64,
        /// Recommended polling interval (seconds)
        interval: u64,
        /// Context data needed for polling
        context: AuthContext,
    },

    /// Code flow - redirect user to authorization URL.
    ///
    /// User should visit `authorization_url`, authorize the app, and get
    /// redirected back with an authorization code. The UI should capture
    /// this code and pass it to `complete()`.
    CodeFlow {
        /// URL to redirect user to for authorization
        authorization_url: String,
        /// State parameter for CSRF protection
        state: String,
        /// Context data needed for completion (PKCE verifier, etc.)
        context: AuthContext,
    },
}

/// Context data for authentication flows.
///
/// This enum provides type-safe storage for flow-specific data needed during
/// polling and completion. Each variant corresponds to a specific
/// authentication method and contains only the fields required for that flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthContext {
    /// API key authentication - no context needed
    ApiKey,

    /// OAuth Device Flow context - for polling
    Device { device_code: String, interval: u64 },

    /// OAuth Code Flow context - for completion
    Code {
        state: String,
        pkce_verifier: Option<String>,
    },
}

impl AuthContext {
    /// Creates a Device flow context
    pub fn device(device_code: String, interval: u64) -> Self {
        Self::Device { device_code, interval }
    }

    /// Creates a Code flow context
    pub fn code(state: String, pkce_verifier: Option<String>) -> Self {
        Self::Code { state, pkce_verifier }
    }

    /// Creates an API key context (no data needed)
    pub fn api_key() -> Self {
        Self::ApiKey
    }

    /// Returns true if this is a Device flow context
    pub fn is_device(&self) -> bool {
        matches!(self, Self::Device { .. })
    }

    /// Returns true if this is a Code flow context
    pub fn is_code(&self) -> bool {
        matches!(self, Self::Code { .. })
    }

    /// Returns true if this is an API key context
    pub fn is_api_key(&self) -> bool {
        matches!(self, Self::ApiKey)
    }
}

/// Result data from successful authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResult {
    /// User provided API key manually with optional URL parameters.
    ///
    /// For simple providers (OpenAI): `url_params` is empty.
    /// For cloud providers (Vertex, Azure): `url_params` contains project_id,
    /// location, etc.
    ApiKey {
        /// The API key itself
        api_key: String,
        /// Additional URL parameters for cloud providers
        url_params: HashMap<String, String>,
    },

    /// OAuth flow completed with tokens.
    OAuthTokens {
        /// Access token for API requests
        access_token: String,
        /// Optional refresh token for getting new access tokens
        refresh_token: Option<String>,
        /// How long the access token is valid (seconds)
        expires_in: Option<u64>,
    },

    /// Authorization code ready for exchange.
    ///
    /// This is returned by `poll_until_complete` for code flows where
    /// the UI manually collects the code from the user. The flow
    /// implementation will exchange this for tokens in `complete()`.
    AuthorizationCode {
        /// Authorization code from OAuth provider
        code: String,
        /// State parameter for CSRF validation
        state: String,
        /// PKCE code verifier (if PKCE was used)
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_device_context_creation() {
        let context = AuthContext::device("ABC123".to_string(), 5);

        assert!(context.is_device());
        assert!(!context.is_code());
        assert!(!context.is_api_key());

        match context {
            AuthContext::Device { device_code, interval } => {
                assert_eq!(device_code, "ABC123");
                assert_eq!(interval, 5);
            }
            _ => panic!("Expected Device variant"),
        }
    }

    #[test]
    fn test_code_context_with_pkce() {
        let context = AuthContext::code("state123".to_string(), Some("verifier456".to_string()));

        assert!(context.is_code());
        assert!(!context.is_device());
        assert!(!context.is_api_key());

        match context {
            AuthContext::Code { state, pkce_verifier } => {
                assert_eq!(state, "state123");
                assert_eq!(pkce_verifier, Some("verifier456".to_string()));
            }
            _ => panic!("Expected Code variant"),
        }
    }

    #[test]
    fn test_code_context_without_pkce() {
        let context = AuthContext::code("state123".to_string(), None);

        match context {
            AuthContext::Code { state, pkce_verifier } => {
                assert_eq!(state, "state123");
                assert!(pkce_verifier.is_none());
            }
            _ => panic!("Expected Code variant"),
        }
    }

    #[test]
    fn test_api_key_context() {
        let context = AuthContext::api_key();

        assert!(context.is_api_key());
        assert!(!context.is_device());
        assert!(!context.is_code());
    }

    #[test]
    fn test_device_serialization() {
        let context = AuthContext::device("ABC123".to_string(), 5);
        let json = serde_json::to_value(&context).unwrap();

        assert_eq!(json["type"], "device");
        assert_eq!(json["device_code"], "ABC123");
        assert_eq!(json["interval"], 5);
    }

    #[test]
    fn test_code_serialization() {
        let context = AuthContext::code("state123".to_string(), Some("verifier456".to_string()));
        let json = serde_json::to_value(&context).unwrap();

        assert_eq!(json["type"], "code");
        assert_eq!(json["state"], "state123");
        assert_eq!(json["pkce_verifier"], "verifier456");
    }

    #[test]
    fn test_api_key_serialization() {
        let context = AuthContext::api_key();
        let json = serde_json::to_value(&context).unwrap();

        assert_eq!(json["type"], "api_key");
        // ApiKey variant has no fields, should only have type
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn test_device_deserialization() {
        let json = r#"{"type":"device","device_code":"ABC123","interval":5}"#;
        let context: AuthContext = serde_json::from_str(json).unwrap();

        match context {
            AuthContext::Device { device_code, interval } => {
                assert_eq!(device_code, "ABC123");
                assert_eq!(interval, 5);
            }
            _ => panic!("Expected Device variant"),
        }
    }

    #[test]
    fn test_code_deserialization() {
        let json = r#"{"type":"code","state":"state123","pkce_verifier":"verifier456"}"#;
        let context: AuthContext = serde_json::from_str(json).unwrap();

        match context {
            AuthContext::Code { state, pkce_verifier } => {
                assert_eq!(state, "state123");
                assert_eq!(pkce_verifier, Some("verifier456".to_string()));
            }
            _ => panic!("Expected Code variant"),
        }
    }

    #[test]
    fn test_context_equality() {
        let ctx1 = AuthContext::device("ABC123".to_string(), 5);
        let ctx2 = AuthContext::device("ABC123".to_string(), 5);
        let ctx3 = AuthContext::device("XYZ789".to_string(), 5);

        assert_eq!(ctx1, ctx2);
        assert_ne!(ctx1, ctx3);
    }

    #[test]
    fn test_auth_method_serialization() {
        let json = serde_json::to_string(&AuthMethod::ApiKey).unwrap();
        assert_eq!(json, r#""api_key""#);

        let oauth_device = AuthMethod::OAuthDevice(OAuthConfig {
            device_code_url: Some("https://example.com/device".to_string()),
            device_token_url: Some("https://example.com/token".to_string()),
            auth_url: None,
            token_url: None,
            client_id: "client-id".to_string(),
            scopes: vec!["read".to_string()],
            redirect_uri: String::new(),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        });
        let json = serde_json::to_string(&oauth_device).unwrap();
        assert!(json.contains("oauth_device"));
        assert!(json.contains("client-id"));

        let oauth_code = AuthMethod::OAuthCode(OAuthConfig {
            device_code_url: None,
            device_token_url: None,
            auth_url: Some("https://example.com/auth".to_string()),
            token_url: Some("https://example.com/token".to_string()),
            client_id: "client-id".to_string(),
            scopes: vec!["read".to_string()],
            redirect_uri: "https://example.com/callback".to_string(),
            use_pkce: true,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        });
        let json = serde_json::to_string(&oauth_code).unwrap();
        assert!(json.contains("oauth_code"));
        assert!(json.contains("client-id"));
    }

    #[test]
    fn test_auth_method_oauth_config() {
        let config = OAuthConfig {
            device_code_url: Some("https://example.com/device".to_string()),
            device_token_url: Some("https://example.com/token".to_string()),
            auth_url: None,
            token_url: None,
            client_id: "client-id".to_string(),
            scopes: vec!["read".to_string()],
            redirect_uri: String::new(),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        };

        let method = AuthMethod::oauth_device(config.clone());
        assert!(method.oauth_config().is_some());

        let api_key_method = AuthMethod::ApiKey;
        assert!(api_key_method.oauth_config().is_none());
    }
}
