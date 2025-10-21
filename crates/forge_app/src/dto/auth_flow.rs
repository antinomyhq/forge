/// Authentication flow types for generic provider authentication.
///
/// This module provides types that support all provider authentication
/// patterns:
/// - Simple API key authentication
/// - OAuth Device Flow
/// - OAuth + API Key Exchange (GitHub Copilot pattern)
/// - OAuth Authorization Code Flow
/// - Cloud Service Account with Parameters (Vertex AI, Azure)
/// - Custom Provider Registration (OpenAI/Anthropic-compatible endpoints)
use std::collections::HashMap;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use super::ProviderResponse;

/// Authentication method type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethodType {
    /// Direct API key entry
    ApiKey,
    /// OAuth device flow (display code to user)
    #[serde(rename = "oauth_device")]
    OAuthDevice,
    /// OAuth authorization code flow (redirect to browser)
    #[serde(rename = "oauth_code")]
    OAuthCode,
}

/// Result of initiating authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthInitiation {
    /// API key auth - prompt user for key and optional parameters.
    ///
    /// For simple providers (OpenAI, Anthropic), `required_params` is empty.
    /// For cloud providers (Vertex AI, Azure), includes parameters like
    /// project_id, location, etc.
    ApiKeyPrompt {
        /// Label for the API key input field
        label: String,
        /// Optional description explaining what the key is for
        description: Option<String>,
        /// Required parameters for cloud providers (project_id, location, etc.)
        /// Empty for simple API key providers (OpenAI, Anthropic)
        required_params: Vec<UrlParameter>,
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

    /// Custom provider registration - prompt for provider details.
    ///
    /// User should provide provider name, base URL, model ID, and optional API
    /// key.
    CustomProviderPrompt {
        /// Compatibility mode for this custom provider
        compatibility_mode: ProviderResponse,
        /// Required parameters for custom provider setup
        required_params: Vec<UrlParameter>,
    },
}

/// Context data needed for polling/completion.
///
/// This is an opaque container for flow-specific data like device codes,
/// session IDs, PKCE verifiers, etc. The data is stored as key-value pairs
/// to keep the types generic.
#[derive(Debug, Clone, Serialize, Deserialize, Default, Setters)]
#[setters(strip_option, into)]
pub struct AuthContext {
    /// Opaque data needed for polling (device_code, session_id, etc.)
    pub polling_data: HashMap<String, String>,

    /// Opaque data needed for completion (PKCE verifier, state, etc.)
    pub completion_data: HashMap<String, String>,
}

/// Result data from successful authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResult {
    /// User provided API key manually with optional URL parameters.
    ///
    /// For simple providers (OpenAI): `url_params` is empty.
    /// For cloud providers (Vertex, Azure): `url_params` contains project_id,
    /// location, etc. For custom providers: `url_params` contains base_url,
    /// model_id, compatibility_mode.
    ApiKey {
        /// The API key itself
        api_key: String,
        /// Additional URL parameters for cloud/custom providers
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

    /// Custom provider registration completed.
    ///
    /// Contains all information needed to create a custom provider credential.
    CustomProvider {
        /// Display name for this provider
        provider_name: String,
        /// API endpoint base URL
        base_url: String,
        /// Model identifier to use
        model_id: String,
        /// Optional API key (not required for local servers)
        api_key: Option<String>,
        /// Compatibility mode (OpenAI or Anthropic)
        compatibility_mode: ProviderResponse,
    },
}

/// URL parameter for providers requiring additional configuration.
///
/// Used for cloud providers (Vertex AI, Azure) that need parameters like
/// project_id, location, etc., and for custom provider registration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct UrlParameter {
    /// Parameter key (e.g., "project_id", "location", "base_url", "model_id")
    pub key: String,
    /// Human-readable label for UI display
    pub label: String,
    /// Optional description explaining what this parameter is
    pub description: Option<String>,
    /// Optional default value to pre-fill
    pub default_value: Option<String>,
    /// Whether this parameter is required
    #[setters(skip)]
    pub required: bool,
    /// Optional validation pattern (regex)
    pub validation_pattern: Option<String>,
}

impl UrlParameter {
    /// Creates a new parameter with default settings.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            required: false,
            ..Default::default()
        }
    }

    /// Creates a new required parameter.
    pub fn required(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            required: true,
            ..Default::default()
        }
    }

    /// Creates a new optional parameter.
    pub fn optional(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            required: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_auth_context_default() {
        let context = AuthContext::default();
        assert!(context.polling_data.is_empty());
        assert!(context.completion_data.is_empty());
    }

    #[test]
    fn test_auth_context_with_polling_data() {
        let mut polling_data = HashMap::new();
        polling_data.insert("device_code".to_string(), "ABC123".to_string());

        let context = AuthContext::default().polling_data(polling_data.clone());
        assert_eq!(context.polling_data, polling_data);
        assert!(context.completion_data.is_empty());
    }

    #[test]
    fn test_auth_context_with_completion_data() {
        let mut completion_data = HashMap::new();
        completion_data.insert("pkce_verifier".to_string(), "XYZ789".to_string());

        let context = AuthContext::default().completion_data(completion_data.clone());
        assert!(context.polling_data.is_empty());
        assert_eq!(context.completion_data, completion_data);
    }

    #[test]
    fn test_url_parameter_builder() {
        let param = UrlParameter::required("project_id", "GCP Project ID")
            .description("Your Google Cloud project ID")
            .validation_pattern(r"^[a-z][a-z0-9-]{4,28}[a-z0-9]$");

        assert_eq!(param.key, "project_id");
        assert_eq!(param.label, "GCP Project ID");
        assert_eq!(
            param.description,
            Some("Your Google Cloud project ID".to_string())
        );
        assert!(param.required);
        assert_eq!(
            param.validation_pattern,
            Some(r"^[a-z][a-z0-9-]{4,28}[a-z0-9]$".to_string())
        );
    }

    #[test]
    fn test_url_parameter_optional() {
        let param = UrlParameter::optional("api_key", "API Key")
            .default_value("default-key")
            .description("Optional API key");

        assert_eq!(param.key, "api_key");
        assert!(!param.required);
        assert_eq!(param.default_value, Some("default-key".to_string()));
    }

    #[test]
    fn test_auth_method_type_serialization() {
        let json = serde_json::to_string(&AuthMethodType::ApiKey).unwrap();
        assert_eq!(json, r#""api_key""#);

        let json = serde_json::to_string(&AuthMethodType::OAuthDevice).unwrap();
        assert_eq!(json, r#""oauth_device""#);
    }
}
