use std::collections::HashMap;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use super::{AuthType, OAuthTokens};
use crate::dto::{ApiKey, URLParam, URLParamValue};

/// Provider credential with support for multiple authentication methods
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct ProviderCredential {
    /// Type of authentication
    pub auth_type: AuthType,

    /// API key (for ApiKey and OAuthWithApiKey auth types)
    pub api_key: Option<ApiKey>,

    /// OAuth tokens (for OAuth and OAuthWithApiKey auth types)
    pub oauth_tokens: Option<OAuthTokens>,

    /// URL parameters (e.g., Azure resource name, Vertex project ID)
    #[serde(default)]
    pub url_params: HashMap<URLParam, URLParamValue>,
}

impl ProviderCredential {
    /// Creates a new API key credential
    pub fn new_api_key(api_key: impl Into<ApiKey>) -> Self {
        Self {
            auth_type: AuthType::ApiKey,
            api_key: Some(api_key.into()),
            oauth_tokens: None,
            url_params: HashMap::new(),
        }
    }

    /// Creates a new OAuth credential
    pub fn new_oauth(oauth_tokens: OAuthTokens) -> Self {
        Self {
            auth_type: AuthType::OAuth,
            api_key: None,
            oauth_tokens: Some(oauth_tokens),
            url_params: HashMap::new(),
        }
    }

    /// Creates a new OAuth+API key credential (GitHub Copilot pattern)
    pub fn new_oauth_with_api_key(api_key: impl Into<ApiKey>, oauth_tokens: OAuthTokens) -> Self {
        Self {
            auth_type: AuthType::OAuthWithApiKey,
            api_key: Some(api_key.into()),
            oauth_tokens: Some(oauth_tokens),
            url_params: HashMap::new(),
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
        self.api_key.as_ref().map(|k| k.as_str())
    }

    /// Gets the OAuth access token if available
    pub fn get_access_token(&self) -> Option<&str> {
        self.oauth_tokens.as_ref().map(|t| t.access_token.as_str())
    }

    /// Updates the OAuth tokens
    pub fn update_oauth_tokens(&mut self, tokens: OAuthTokens) {
        self.oauth_tokens = Some(tokens);
    }
}
