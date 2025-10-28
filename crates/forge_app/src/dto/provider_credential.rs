use std::collections::HashMap;

/// Domain models for provider credentials with OAuth support
use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use super::ProviderId;
use crate::dto::{URLParam, URLParamValue};

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
    pub fn new(
        refresh_token: impl Into<String>,
        access_token: impl Into<String>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            refresh_token: refresh_token.into(),
            access_token: access_token.into(),
            expires_at,
        }
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
    pub url_params: HashMap<URLParam, URLParamValue>,

    /// When the credential was created
    pub created_at: DateTime<Utc>,

    /// When the credential was last updated
    pub updated_at: DateTime<Utc>,
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
}
