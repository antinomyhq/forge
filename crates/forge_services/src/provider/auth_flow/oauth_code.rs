//! OAuth Authorization Code Flow
//!
//! This flow is used for web-based OAuth providers that require browser
//! redirects. Supports PKCE (Proof Key for Code Exchange) for enhanced
//! security.
//!
//! # Flow Steps
//! 1. `initiate()` - Generate authorization URL with PKCE challenge, return URL
//!    + state
//! 2. `poll_until_complete()` - Not applicable (user manually pastes code),
//!    returns error
//! 3. `complete()` - Exchange authorization code for tokens using PKCE verifier
//! 4. `refresh()` - Use refresh token to get new access token
//! 5. `validate()` - Check if token is expired

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_app::dto::{
    AuthContext, AuthInitiation, AuthResult, OAuthConfig, OAuthTokens, ProviderCredential,
    ProviderId,
};

use super::AuthenticationFlow;
use super::error::AuthFlowError;
use crate::provider::oauth::ForgeOAuthService;

/// OAuth authorization code flow with PKCE support
///
/// This flow generates an authorization URL that the user visits in their
/// browser. After authorization, the provider redirects back with an
/// authorization code. The code is then exchanged for access and refresh
/// tokens.
///
/// # PKCE Security
/// PKCE (RFC 7636) is used to prevent authorization code interception attacks.
/// A code verifier is generated during initiation and used during token
/// exchange.
pub struct OAuthCodeFlow {
    /// Provider identifier
    provider_id: ProviderId,
    /// OAuth configuration
    config: OAuthConfig,
    /// OAuth service for HTTP requests
    oauth_service: Arc<ForgeOAuthService>,
}

impl OAuthCodeFlow {
    /// Creates a new OAuth authorization code flow
    ///
    /// # Arguments
    /// * `provider_id` - Provider identifier
    /// * `config` - OAuth configuration (auth_url, token_url, client_id,
    ///   redirect_uri, use_pkce, scopes)
    /// * `oauth_service` - OAuth service for making HTTP requests
    pub fn new(
        provider_id: ProviderId,
        config: OAuthConfig,
        oauth_service: Arc<ForgeOAuthService>,
    ) -> Self {
        Self { provider_id, config, oauth_service }
    }
}

#[async_trait]
impl AuthenticationFlow for OAuthCodeFlow {
    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        // Build authorization URL with PKCE
        let auth_params = self
            .oauth_service
            .build_auth_code_url(&self.config)
            .map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Failed to build auth URL: {}", e))
            })?;

        // Store PKCE verifier and state in completion_data
        let mut completion_data = HashMap::new();
        completion_data.insert("state".to_string(), auth_params.state.clone());
        if let Some(verifier) = &auth_params.code_verifier {
            completion_data.insert("code_verifier".to_string(), verifier.clone());
        }

        let context = AuthContext { polling_data: HashMap::new(), completion_data };

        Ok(AuthInitiation::CodeFlow {
            authorization_url: auth_params.auth_url,
            state: auth_params.state,
            context,
        })
    }

    async fn poll_until_complete(
        &self,
        _context: &AuthContext,
        _timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError> {
        // OAuth code flow requires manual code entry, no polling
        Err(AuthFlowError::PollFailed(
            "OAuth code flow requires manual authorization code entry. Use complete() instead."
                .to_string(),
        ))
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match result {
            AuthResult::AuthorizationCode { code, state: _, code_verifier } => {
                // Exchange code for tokens with PKCE verifier (if provided)
                // Note: For Anthropic, the code is in format "code#state" which is handled
                // by the exchange_auth_code method
                let token_response = self
                    .oauth_service
                    .exchange_auth_code(&self.config, &code, code_verifier.as_deref())
                    .await
                    .map_err(|e| {
                        AuthFlowError::CompletionFailed(format!(
                            "Failed to exchange authorization code: {}",
                            e
                        ))
                    })?;

                // Calculate expiry time
                let expires_at = if let Some(expires_in) = token_response.expires_in {
                    chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64)
                } else {
                    // Default to 1 hour if not provided
                    chrono::Utc::now() + chrono::Duration::hours(1)
                };

                // Create OAuth tokens
                let oauth_tokens = if let Some(refresh_token) = token_response.refresh_token {
                    OAuthTokens::new(refresh_token, token_response.access_token, expires_at)
                } else {
                    // For providers without refresh token, use access token as refresh token
                    OAuthTokens::new(
                        token_response.access_token.clone(),
                        token_response.access_token,
                        expires_at,
                    )
                };

                // Create credential
                let credential =
                    ProviderCredential::new_oauth(self.provider_id.clone(), oauth_tokens);

                Ok(credential)
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "Expected AuthorizationCode result for OAuth code flow".to_string(),
            )),
        }
    }

    async fn refresh(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        // Get stored OAuth tokens
        let oauth_tokens = credential.oauth_tokens.as_ref().ok_or_else(|| {
            AuthFlowError::RefreshFailed("Missing OAuth tokens in credential".to_string())
        })?;

        // Use refresh token to get new access token
        let token_response = self
            .oauth_service
            .refresh_access_token(&self.config, &oauth_tokens.refresh_token)
            .await
            .map_err(|e| {
                AuthFlowError::RefreshFailed(format!("Failed to refresh access token: {}", e))
            })?;

        // Calculate new expiry time
        let expires_at = if let Some(expires_in) = token_response.expires_in {
            chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64)
        } else {
            chrono::Utc::now() + chrono::Duration::hours(1)
        };

        // Create updated OAuth tokens
        let updated_tokens = OAuthTokens::new(
            oauth_tokens.refresh_token.clone(), // Keep original refresh token
            token_response.access_token,
            expires_at,
        );

        // Create new credential with refreshed tokens
        let mut refreshed = credential.clone();
        refreshed.oauth_tokens = Some(updated_tokens);
        refreshed.updated_at = chrono::Utc::now();

        Ok(refreshed)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn fixture_oauth_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "test-client-id".to_string(),
            device_code_url: None,
            device_token_url: None,
            auth_url: Some("https://provider.com/authorize".to_string()),
            token_url: Some("https://provider.com/token".to_string()),
            scopes: vec!["read:user".to_string(), "repo".to_string()],
            redirect_uri: "https://myapp.com/callback".to_string(),
            use_pkce: true,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        }
    }

    #[tokio::test]
    async fn test_initiate_generates_auth_url() {
        let config = fixture_oauth_config();
        let oauth_service = Arc::new(ForgeOAuthService::new());

        let flow = OAuthCodeFlow::new(ProviderId::OpenAI, config, oauth_service);

        let result = flow.initiate().await;

        assert!(result.is_ok());

        if let AuthInitiation::CodeFlow { authorization_url, state, context } = result.unwrap() {
            // Verify URL contains expected parameters
            assert!(authorization_url.contains("client_id=test-client-id"));
            assert!(authorization_url.contains("response_type=code"));
            assert!(authorization_url.contains("redirect_uri=https%3A%2F%2Fmyapp.com%2Fcallback"));
            assert!(authorization_url.contains("scope=read%3Auser+repo"));

            // PKCE challenge should be present
            assert!(authorization_url.contains("code_challenge="));
            assert!(authorization_url.contains("code_challenge_method=S256"));

            // State should not be empty
            assert!(!state.is_empty());

            // Context should contain state and code_verifier
            assert!(context.completion_data.contains_key("state"));
            assert!(context.completion_data.contains_key("code_verifier"));
        } else {
            panic!("Expected CodeFlow initiation");
        }
    }

    #[tokio::test]
    async fn test_poll_returns_error() {
        let config = fixture_oauth_config();
        let oauth_service = Arc::new(ForgeOAuthService::new());

        let flow = OAuthCodeFlow::new(ProviderId::OpenAI, config, oauth_service);

        let context = AuthContext {
            polling_data: HashMap::new(),
            completion_data: HashMap::new(),
        };

        let result = flow
            .poll_until_complete(&context, Duration::from_secs(60))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("manual"));
    }

    #[tokio::test]
    async fn test_complete_with_wrong_result_type() {
        let config = fixture_oauth_config();
        let oauth_service = Arc::new(ForgeOAuthService::new());

        let flow = OAuthCodeFlow::new(ProviderId::OpenAI, config, oauth_service);

        let result =
            AuthResult::ApiKey { api_key: "test-key".to_string(), url_params: HashMap::new() };

        let actual = flow.complete(result).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Expected AuthorizationCode")
        );
    }
}
