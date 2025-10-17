/// Authentication method definitions for providers
///
/// This module defines the types and structures for declaring multiple
/// authentication methods per provider (API Key, OAuth Device Flow, OAuth Code
/// Flow).
use serde::{Deserialize, Serialize};

/// Type of authentication method available for a provider
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethodType {
    /// Traditional API key authentication
    ApiKey,

    /// OAuth device authorization flow (GitHub Copilot pattern)
    /// User visits URL, enters code, CLI polls for completion
    OAuthDevice,

    /// OAuth authorization code flow with manual paste (Anthropic pattern)
    /// User visits URL, authorizes, manually pastes code back to CLI
    OAuthCode,

    /// OAuth flow that results in API key creation
    /// Opens browser to provider's API key creation page
    OAuthApiKey,
}

/// Authentication method configuration for a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMethod {
    /// Type of authentication method
    pub method_type: AuthMethodType,

    /// Human-readable label for UI display
    /// Examples: "API Key", "GitHub OAuth", "Claude Pro/Max"
    pub label: String,

    /// Optional description explaining when to use this method
    pub description: Option<String>,

    /// OAuth-specific configuration (required for OAuth methods)
    #[serde(default)]
    pub oauth_config: Option<OAuthConfig>,
}

impl AuthMethod {
    /// Creates a new API key authentication method
    pub fn api_key(label: impl Into<String>, description: Option<String>) -> Self {
        Self {
            method_type: AuthMethodType::ApiKey,
            label: label.into(),
            description,
            oauth_config: None,
        }
    }

    /// Creates a new OAuth device flow authentication method
    pub fn oauth_device(
        label: impl Into<String>,
        description: Option<String>,
        config: OAuthConfig,
    ) -> Self {
        Self {
            method_type: AuthMethodType::OAuthDevice,
            label: label.into(),
            description,
            oauth_config: Some(config),
        }
    }

    /// Creates a new OAuth code flow authentication method
    pub fn oauth_code(
        label: impl Into<String>,
        description: Option<String>,
        config: OAuthConfig,
    ) -> Self {
        Self {
            method_type: AuthMethodType::OAuthCode,
            label: label.into(),
            description,
            oauth_config: Some(config),
        }
    }

    /// Creates a new OAuth API key method (browser-assisted)
    pub fn oauth_api_key(label: impl Into<String>, description: Option<String>) -> Self {
        Self {
            method_type: AuthMethodType::OAuthApiKey,
            label: label.into(),
            description,
            oauth_config: None,
        }
    }
}

/// OAuth configuration for device and code flows
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl OAuthConfig {
    /// Creates a new OAuth device flow configuration
    pub fn device_flow(
        device_code_url: impl Into<String>,
        device_token_url: impl Into<String>,
        client_id: impl Into<String>,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            device_code_url: Some(device_code_url.into()),
            device_token_url: Some(device_token_url.into()),
            auth_url: None,
            token_url: None,
            client_id: client_id.into(),
            scopes,
            redirect_uri: String::new(),
            use_pkce: false,
            token_refresh_url: None,
        }
    }

    /// Creates a new OAuth authorization code flow configuration
    pub fn code_flow(
        auth_url: impl Into<String>,
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        scopes: Vec<String>,
        redirect_uri: impl Into<String>,
        use_pkce: bool,
    ) -> Self {
        Self {
            device_code_url: None,
            device_token_url: None,
            auth_url: Some(auth_url.into()),
            token_url: Some(token_url.into()),
            client_id: client_id.into(),
            scopes,
            redirect_uri: redirect_uri.into(),
            use_pkce,
            token_refresh_url: None,
        }
    }

    /// Sets the token refresh URL (for GitHub Copilot pattern)
    pub fn with_token_refresh_url(mut self, url: impl Into<String>) -> Self {
        self.token_refresh_url = Some(url.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_auth_method_api_key() {
        let method = AuthMethod::api_key("API Key", Some("Use your API key".to_string()));

        assert_eq!(method.method_type, AuthMethodType::ApiKey);
        assert_eq!(method.label, "API Key");
        assert_eq!(method.description, Some("Use your API key".to_string()));
        assert!(method.oauth_config.is_none());
    }

    #[test]
    fn test_auth_method_oauth_device() {
        let config = OAuthConfig::device_flow(
            "https://example.com/device",
            "https://example.com/token",
            "client-id",
            vec!["read".to_string()],
        );

        let method = AuthMethod::oauth_device("GitHub OAuth", None, config.clone());

        assert_eq!(method.method_type, AuthMethodType::OAuthDevice);
        assert_eq!(method.label, "GitHub OAuth");
        assert!(method.oauth_config.is_some());

        let oauth = method.oauth_config.unwrap();
        assert_eq!(oauth.client_id, "client-id");
        assert_eq!(oauth.scopes, vec!["read"]);
    }

    #[test]
    fn test_auth_method_oauth_code() {
        let config = OAuthConfig::code_flow(
            "https://example.com/authorize",
            "https://example.com/token",
            "client-id",
            vec!["user:profile".to_string()],
            "https://example.com/callback",
            true,
        );

        let method = AuthMethod::oauth_code("Claude Pro", None, config);

        assert_eq!(method.method_type, AuthMethodType::OAuthCode);
        assert!(method.oauth_config.is_some());

        let oauth = method.oauth_config.unwrap();
        assert!(oauth.use_pkce);
        assert_eq!(oauth.redirect_uri, "https://example.com/callback");
    }

    #[test]
    fn test_oauth_config_with_token_refresh() {
        let config = OAuthConfig::device_flow(
            "https://github.com/login/device/code",
            "https://github.com/login/oauth/access_token",
            "Iv1.test",
            vec!["read:user".to_string()],
        )
        .with_token_refresh_url("https://api.github.com/copilot_internal/v2/token");

        assert_eq!(
            config.token_refresh_url,
            Some("https://api.github.com/copilot_internal/v2/token".to_string())
        );
    }

    #[test]
    fn test_auth_method_serialization() {
        let method = AuthMethod::api_key("Test", None);
        let json = serde_json::to_string(&method).unwrap();
        let deserialized: AuthMethod = serde_json::from_str(&json).unwrap();

        assert_eq!(method.method_type, deserialized.method_type);
        assert_eq!(method.label, deserialized.label);
    }

    #[test]
    fn test_oauth_config_device_flow() {
        let config = OAuthConfig::device_flow(
            "https://provider.com/device",
            "https://provider.com/token",
            "test-client",
            vec!["scope1".to_string(), "scope2".to_string()],
        );

        assert_eq!(
            config.device_code_url,
            Some("https://provider.com/device".to_string())
        );
        assert_eq!(
            config.device_token_url,
            Some("https://provider.com/token".to_string())
        );
        assert!(config.auth_url.is_none());
        assert!(config.token_url.is_none());
        assert_eq!(config.client_id, "test-client");
        assert_eq!(config.scopes.len(), 2);
    }

    #[test]
    fn test_oauth_config_code_flow() {
        let config = OAuthConfig::code_flow(
            "https://provider.com/auth",
            "https://provider.com/token",
            "test-client",
            vec!["profile".to_string()],
            "https://provider.com/callback",
            true,
        );

        assert!(config.device_code_url.is_none());
        assert!(config.device_token_url.is_none());
        assert_eq!(
            config.auth_url,
            Some("https://provider.com/auth".to_string())
        );
        assert_eq!(
            config.token_url,
            Some("https://provider.com/token".to_string())
        );
        assert!(config.use_pkce);
        assert_eq!(config.redirect_uri, "https://provider.com/callback");
    }
}
