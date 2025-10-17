use std::collections::HashMap;
use std::str::FromStr;

/// Domain models for provider credentials with OAuth support
use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use super::ProviderId;

/// Type of authentication used for a provider
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    /// Traditional API key authentication
    ApiKey,

    /// OAuth authentication (device or code flow)
    OAuth,

    /// OAuth token used to fetch an API key (GitHub Copilot pattern)
    OAuthWithApiKey,
}

impl AuthType {
    /// Returns string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthType::ApiKey => "api_key",
            AuthType::OAuth => "oauth",
            AuthType::OAuthWithApiKey => "oauth_with_api_key",
        }
    }
}

impl FromStr for AuthType {
    type Err = String;

    /// Parses auth type from database string
    ///
    /// # Errors
    ///
    /// Returns error if string doesn't match any known auth type
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "api_key" => Ok(AuthType::ApiKey),
            "oauth" => Ok(AuthType::OAuth),
            "oauth_with_api_key" => Ok(AuthType::OAuthWithApiKey),
            _ => Err(format!("Unknown auth type: {}", s)),
        }
    }
}

/// OAuth tokens for providers using OAuth authentication
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct OAuthTokens {
    /// Long-lived token for getting new access tokens
    pub refresh_token: String,

    /// Short-lived token for API requests
    pub access_token: String,

    /// When the access token expires
    pub expires_at: DateTime<Utc>,
}

impl OAuthTokens {
    /// Creates new OAuth tokens
    pub fn new(refresh_token: String, access_token: String, expires_at: DateTime<Utc>) -> Self {
        Self { refresh_token, access_token, expires_at }
    }

    /// Checks if the access token is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Checks if token will expire within the given duration
    pub fn expires_within(&self, seconds: i64) -> bool {
        let threshold = Utc::now() + chrono::Duration::seconds(seconds);
        self.expires_at <= threshold
    }
}

/// Provider credential with support for multiple authentication methods
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct ProviderCredential {
    /// Provider identifier
    pub provider_id: ProviderId,

    /// Type of authentication
    pub auth_type: AuthType,

    /// API key (for ApiKey and OAuthWithApiKey auth types)
    pub api_key: Option<String>,

    /// OAuth tokens (for OAuth and OAuthWithApiKey auth types)
    pub oauth_tokens: Option<OAuthTokens>,

    /// URL parameters (e.g., Azure resource name, Vertex project ID)
    #[serde(default)]
    pub url_params: HashMap<String, String>,

    /// When the credential was created
    pub created_at: DateTime<Utc>,

    /// When the credential was last updated
    pub updated_at: DateTime<Utc>,

    /// When the credential was last successfully verified
    pub last_verified_at: Option<DateTime<Utc>>,

    /// Whether the credential is active
    pub is_active: bool,
}

impl ProviderCredential {
    /// Creates a new API key credential
    pub fn new_api_key(provider_id: ProviderId, api_key: String) -> Self {
        let now = Utc::now();
        Self {
            provider_id,
            auth_type: AuthType::ApiKey,
            api_key: Some(api_key),
            oauth_tokens: None,
            url_params: HashMap::new(),
            created_at: now,
            updated_at: now,
            last_verified_at: None,
            is_active: true,
        }
    }

    /// Creates a new OAuth credential
    pub fn new_oauth(provider_id: ProviderId, oauth_tokens: OAuthTokens) -> Self {
        let now = Utc::now();
        Self {
            provider_id,
            auth_type: AuthType::OAuth,
            api_key: None,
            oauth_tokens: Some(oauth_tokens),
            url_params: HashMap::new(),
            created_at: now,
            updated_at: now,
            last_verified_at: None,
            is_active: true,
        }
    }

    /// Creates a new OAuth+API key credential (GitHub Copilot pattern)
    pub fn new_oauth_with_api_key(
        provider_id: ProviderId,
        api_key: String,
        oauth_tokens: OAuthTokens,
    ) -> Self {
        let now = Utc::now();
        Self {
            provider_id,
            auth_type: AuthType::OAuthWithApiKey,
            api_key: Some(api_key),
            oauth_tokens: Some(oauth_tokens),
            url_params: HashMap::new(),
            created_at: now,
            updated_at: now,
            last_verified_at: None,
            is_active: true,
        }
    }

    /// Checks if OAuth tokens need refresh
    pub fn needs_token_refresh(&self) -> bool {
        if let Some(tokens) = &self.oauth_tokens {
            // Refresh if expired or expires within 5 minutes
            tokens.expires_within(300)
        } else {
            false
        }
    }

    /// Gets the API key if available
    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Gets the OAuth access token if available
    pub fn get_access_token(&self) -> Option<&str> {
        self.oauth_tokens.as_ref().map(|t| t.access_token.as_str())
    }

    /// Updates the OAuth tokens
    pub fn update_oauth_tokens(&mut self, tokens: OAuthTokens) {
        self.oauth_tokens = Some(tokens);
        self.updated_at = Utc::now();
    }

    /// Marks the credential as verified
    pub fn mark_verified(&mut self) {
        self.last_verified_at = Some(Utc::now());
    }
}

impl Default for ProviderCredential {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            provider_id: ProviderId::OpenAI,
            auth_type: AuthType::ApiKey,
            api_key: None,
            oauth_tokens: None,
            url_params: HashMap::new(),
            created_at: now,
            updated_at: now,
            last_verified_at: None,
            is_active: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_auth_type_serialization() {
        assert_eq!(AuthType::ApiKey.as_str(), "api_key");
        assert_eq!(AuthType::OAuth.as_str(), "oauth");
        assert_eq!(AuthType::OAuthWithApiKey.as_str(), "oauth_with_api_key");
    }

    #[test]
    fn test_auth_type_parsing() {
        assert_eq!(AuthType::from_str("api_key").unwrap(), AuthType::ApiKey);
        assert_eq!(AuthType::from_str("oauth").unwrap(), AuthType::OAuth);
        assert_eq!(
            AuthType::from_str("oauth_with_api_key").unwrap(),
            AuthType::OAuthWithApiKey
        );
        assert!(AuthType::from_str("unknown").is_err());
    }

    #[test]
    fn test_oauth_tokens_expiration() {
        let expired = OAuthTokens::new(
            "refresh".to_string(),
            "access".to_string(),
            Utc::now() - chrono::Duration::minutes(1),
        );
        assert!(expired.is_expired());

        let valid = OAuthTokens::new(
            "refresh".to_string(),
            "access".to_string(),
            Utc::now() + chrono::Duration::hours(1),
        );
        assert!(!valid.is_expired());
    }

    #[test]
    fn test_oauth_tokens_expires_within() {
        let tokens = OAuthTokens::new(
            "refresh".to_string(),
            "access".to_string(),
            Utc::now() + chrono::Duration::minutes(3),
        );

        assert!(tokens.expires_within(300)); // 5 minutes
        assert!(!tokens.expires_within(60)); // 1 minute
    }

    #[test]
    fn test_new_api_key_credential() {
        let cred = ProviderCredential::new_api_key(ProviderId::OpenAI, "sk-test".to_string());

        assert_eq!(cred.provider_id, ProviderId::OpenAI);
        assert_eq!(cred.auth_type, AuthType::ApiKey);
        assert_eq!(cred.api_key, Some("sk-test".to_string()));
        assert!(cred.oauth_tokens.is_none());
        assert!(cred.is_active);
    }

    #[test]
    fn test_new_oauth_credential() {
        let tokens = OAuthTokens::new(
            "refresh_token".to_string(),
            "access_token".to_string(),
            Utc::now() + chrono::Duration::hours(1),
        );

        let cred = ProviderCredential::new_oauth(ProviderId::Anthropic, tokens.clone());

        assert_eq!(cred.provider_id, ProviderId::Anthropic);
        assert_eq!(cred.auth_type, AuthType::OAuth);
        assert!(cred.api_key.is_none());
        assert_eq!(cred.oauth_tokens, Some(tokens));
    }

    #[test]
    fn test_new_oauth_with_api_key_credential() {
        let tokens = OAuthTokens::new(
            "refresh_token".to_string(),
            "access_token".to_string(),
            Utc::now() + chrono::Duration::hours(1),
        );

        let cred = ProviderCredential::new_oauth_with_api_key(
            ProviderId::OpenAI,
            "api_key".to_string(),
            tokens.clone(),
        );

        assert_eq!(cred.provider_id, ProviderId::OpenAI);
        assert_eq!(cred.auth_type, AuthType::OAuthWithApiKey);
        assert_eq!(cred.api_key, Some("api_key".to_string()));
        assert_eq!(cred.oauth_tokens, Some(tokens));
    }

    #[test]
    fn test_needs_token_refresh() {
        let expired_tokens = OAuthTokens::new(
            "refresh".to_string(),
            "access".to_string(),
            Utc::now() - chrono::Duration::minutes(1),
        );

        let mut cred = ProviderCredential::new_oauth(ProviderId::Anthropic, expired_tokens);
        assert!(cred.needs_token_refresh());

        let fresh_tokens = OAuthTokens::new(
            "refresh".to_string(),
            "access".to_string(),
            Utc::now() + chrono::Duration::hours(1),
        );

        cred.update_oauth_tokens(fresh_tokens);
        assert!(!cred.needs_token_refresh());
    }

    #[test]
    fn test_get_api_key() {
        let cred = ProviderCredential::new_api_key(ProviderId::OpenAI, "sk-test".to_string());
        assert_eq!(cred.get_api_key(), Some("sk-test"));
    }

    #[test]
    fn test_get_access_token() {
        let tokens = OAuthTokens::new(
            "refresh".to_string(),
            "access".to_string(),
            Utc::now() + chrono::Duration::hours(1),
        );

        let cred = ProviderCredential::new_oauth(ProviderId::Anthropic, tokens);
        assert_eq!(cred.get_access_token(), Some("access"));
    }

    #[test]
    fn test_update_oauth_tokens() {
        let old_tokens = OAuthTokens::new(
            "old_refresh".to_string(),
            "old_access".to_string(),
            Utc::now() + chrono::Duration::hours(1),
        );

        let mut cred = ProviderCredential::new_oauth(ProviderId::Anthropic, old_tokens);
        let old_updated_at = cred.updated_at;

        // Sleep briefly to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        let new_tokens = OAuthTokens::new(
            "new_refresh".to_string(),
            "new_access".to_string(),
            Utc::now() + chrono::Duration::hours(2),
        );

        cred.update_oauth_tokens(new_tokens);

        assert_eq!(cred.get_access_token(), Some("new_access"));
        assert!(cred.updated_at > old_updated_at);
    }

    #[test]
    fn test_mark_verified() {
        let mut cred = ProviderCredential::new_api_key(ProviderId::OpenAI, "sk-test".to_string());

        assert!(cred.last_verified_at.is_none());
        cred.mark_verified();
        assert!(cred.last_verified_at.is_some());
    }

    #[test]
    fn test_setters() {
        let cred = ProviderCredential::default()
            .provider_id(ProviderId::Anthropic)
            .api_key("test-key")
            .is_active(false);

        assert_eq!(cred.provider_id, ProviderId::Anthropic);
        assert_eq!(cred.api_key, Some("test-key".to_string()));
        assert!(!cred.is_active);
    }
}
