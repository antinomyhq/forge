//! OAuth Device Flow with API Key Exchange
//!
//! This flow is used by providers like GitHub Copilot that require:
//! 1. Standard OAuth device flow to get OAuth tokens
//! 2. Exchange OAuth access token for a time-limited API key
//! 3. Use the API key for actual API requests
//! 4. Refresh: Re-exchange OAuth token for fresh API key when it expires

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_app::dto::{
    AuthContext, AuthInitiation, AuthMethodType, AuthResult, OAuthTokens, ProviderCredential,
    ProviderId,
};

use super::AuthenticationFlow;
use super::error::AuthFlowError;
use crate::provider::OAuthConfig;
use crate::provider::github_copilot::GitHubCopilotService;
use crate::provider::oauth::ForgeOAuthService;

/// OAuth device flow that exchanges tokens for API keys (GitHub Copilot
/// pattern)
///
/// This flow combines standard OAuth device authorization with
/// provider-specific API key exchange. The OAuth token is stored for refresh,
/// but the API key is what's actually used in requests.
///
/// # Flow Steps
/// 1. `initiate()` - Start OAuth device flow, return user code + verification
///    URL
/// 2. `poll_until_complete()` - Poll until user authorizes, get OAuth tokens
/// 3. `complete()` - Exchange OAuth token for API key, create credential
/// 4. `refresh()` - Use stored OAuth token to fetch fresh API key
pub struct OAuthWithApiKeyFlow {
    /// Provider identifier
    provider_id: ProviderId,
    /// OAuth configuration
    config: OAuthConfig,
    /// OAuth service for device flow
    oauth_service: Arc<ForgeOAuthService>,
    /// GitHub Copilot service for token exchange
    copilot_service: Arc<GitHubCopilotService>,
}

impl OAuthWithApiKeyFlow {
    /// Creates a new OAuth with API key flow
    ///
    /// # Arguments
    /// * `provider_id` - Provider identifier (e.g., ProviderId::GithubCopilot)
    /// * `config` - OAuth configuration (device_code_url, device_token_url,
    ///   client_id, scopes)
    /// * `oauth_service` - OAuth service for device authorization
    /// * `copilot_service` - GitHub Copilot service for token-to-API-key
    ///   exchange
    pub fn new(
        provider_id: ProviderId,
        config: OAuthConfig,
        oauth_service: Arc<ForgeOAuthService>,
        copilot_service: Arc<GitHubCopilotService>,
    ) -> Self {
        Self { provider_id, config, oauth_service, copilot_service }
    }
}

#[async_trait]
impl AuthenticationFlow for OAuthWithApiKeyFlow {
    fn auth_method_type(&self) -> AuthMethodType {
        AuthMethodType::OAuthDevice
    }

    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        // Validate configuration
        let device_code_url = self.config.device_code_url.as_ref().ok_or_else(|| {
            AuthFlowError::InitiationFailed("device_code_url not configured".to_string())
        })?;
        let device_token_url = self.config.device_token_url.as_ref().ok_or_else(|| {
            AuthFlowError::InitiationFailed("device_token_url not configured".to_string())
        })?;

        // Build oauth2 client
        use oauth2::basic::BasicClient;
        use oauth2::{ClientId, DeviceAuthorizationUrl, Scope, TokenUrl};

        let client = BasicClient::new(ClientId::new(self.config.client_id.clone()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(device_code_url.clone()).map_err(|e| {
                    AuthFlowError::InitiationFailed(format!("Invalid device_code_url: {}", e))
                })?,
            )
            .set_token_uri(TokenUrl::new(device_token_url.clone()).map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Invalid device_token_url: {}", e))
            })?);

        // Request device authorization
        let mut request = client.exchange_device_code();
        for scope in &self.config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }

        // Build HTTP client with custom headers
        let http_client = self
            .oauth_service
            .build_http_client(self.config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        let http_fn =
            |req| ForgeOAuthService::github_compliant_http_request(http_client.clone(), req);

        let device_auth_response: oauth2::StandardDeviceAuthorizationResponse =
            request.request_async(&http_fn).await.map_err(|e| {
                AuthFlowError::InitiationFailed(format!(
                    "Device authorization request failed: {}",
                    e
                ))
            })?;

        // Build context with device code for polling
        let mut polling_data = HashMap::new();
        polling_data.insert(
            "device_code".to_string(),
            device_auth_response.device_code().secret().to_string(),
        );

        let context = AuthContext::default().polling_data(polling_data);

        Ok(AuthInitiation::DeviceFlow {
            user_code: device_auth_response.user_code().secret().to_string(),
            verification_uri: device_auth_response.verification_uri().to_string(),
            verification_uri_complete: device_auth_response
                .verification_uri_complete()
                .map(|u| u.secret().to_string()),
            expires_in: device_auth_response.expires_in().as_secs(),
            interval: device_auth_response.interval().as_secs(),
            context,
        })
    }

    async fn poll_until_complete(
        &self,
        context: &AuthContext,
        timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError> {
        // Extract device_code from context
        let device_code = context.polling_data.get("device_code").ok_or_else(|| {
            AuthFlowError::PollFailed("Missing device_code in context".to_string())
        })?;

        let device_token_url = self.config.device_token_url.as_ref().ok_or_else(|| {
            AuthFlowError::PollFailed("device_token_url not configured".to_string())
        })?;

        // Build HTTP client for manual polling
        let http_client = self
            .oauth_service
            .build_http_client(self.config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

        let start_time = tokio::time::Instant::now();
        let interval = Duration::from_secs(5); // Default interval

        loop {
            // Check timeout
            if start_time.elapsed() >= timeout {
                return Err(AuthFlowError::Timeout(timeout));
            }

            // Wait before polling
            tokio::time::sleep(interval).await;

            // Build token request
            let params = vec![
                (
                    "grant_type".to_string(),
                    "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                ),
                ("device_code".to_string(), device_code.clone()),
                ("client_id".to_string(), self.config.client_id.clone()),
            ];

            let body = serde_urlencoded::to_string(&params).map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to encode request: {}", e))
            })?;

            // Make HTTP request
            let mut headers = HeaderMap::new();
            headers.insert(
                "Content-Type",
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
            headers.insert("Accept", HeaderValue::from_static("application/json"));

            // Add custom headers
            if let Some(custom_headers) = &self.config.custom_headers {
                for (key, value) in custom_headers {
                    if let (Ok(name), Ok(val)) =
                        (HeaderName::try_from(key), HeaderValue::from_str(value))
                    {
                        headers.insert(name, val);
                    }
                }
            }

            let response = http_client
                .post(device_token_url)
                .headers(headers)
                .body(body)
                .send()
                .await
                .map_err(|e| AuthFlowError::PollFailed(format!("HTTP request failed: {}", e)))?;

            let status = response.status();
            let body_text = response.text().await.map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to read response: {}", e))
            })?;

            // Parse response
            if status.is_success() {
                // Success - parse token response
                let token_response: serde_json::Value =
                    serde_json::from_str(&body_text).map_err(|e| {
                        AuthFlowError::PollFailed(format!("Failed to parse token response: {}", e))
                    })?;

                // GitHub returns HTTP 200 with error field for pending/denied/expired
                if let Some(error) = token_response["error"].as_str() {
                    match error {
                        "authorization_pending" => {
                            // Still waiting for user authorization - continue polling
                            tokio::time::sleep(interval).await;
                            continue;
                        }
                        "slow_down" => {
                            // Server requests slower polling
                            tokio::time::sleep(interval * 2).await;
                            continue;
                        }
                        "expired_token" => {
                            return Err(AuthFlowError::Expired);
                        }
                        "access_denied" => {
                            return Err(AuthFlowError::Denied);
                        }
                        _ => {
                            return Err(AuthFlowError::PollFailed(format!(
                                "OAuth error: {}",
                                error
                            )));
                        }
                    }
                }

                // Success - extract access token
                let access_token = token_response["access_token"]
                    .as_str()
                    .ok_or_else(|| {
                        AuthFlowError::PollFailed(format!(
                            "Missing access_token in response: {}",
                            body_text
                        ))
                    })?
                    .to_string();

                let refresh_token = token_response["refresh_token"]
                    .as_str()
                    .map(|s| s.to_string());
                let expires_in = token_response["expires_in"].as_u64();

                return Ok(AuthResult::OAuthTokens { access_token, refresh_token, expires_in });
            }

            // Parse error response
            let error_response: serde_json::Value =
                serde_json::from_str(&body_text).unwrap_or_else(|_| {
                    serde_json::json!({"error": "unknown_error", "error_description": body_text})
                });

            let error_code = error_response["error"].as_str().unwrap_or("unknown_error");

            match error_code {
                "authorization_pending" => {
                    // User hasn't authorized yet, continue polling
                    continue;
                }
                "slow_down" => {
                    // Server requests slower polling
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                "expired_token" => {
                    return Err(AuthFlowError::Expired);
                }
                "access_denied" => {
                    return Err(AuthFlowError::Denied);
                }
                _ => {
                    let description = error_response["error_description"]
                        .as_str()
                        .unwrap_or("Unknown error");
                    return Err(AuthFlowError::PollFailed(format!(
                        "{}: {}",
                        error_code, description
                    )));
                }
            }
        }
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match result {
            AuthResult::OAuthTokens { access_token, refresh_token, expires_in: _ } => {
                // Step 2: Exchange OAuth token for API key
                let (api_key, expires_at) = self
                    .copilot_service
                    .get_copilot_api_key(&access_token)
                    .await
                    .map_err(|e| {
                        AuthFlowError::CompletionFailed(format!(
                            "Failed to exchange OAuth token for API key: {}",
                            e
                        ))
                    })?;

                // Create OAuth tokens structure
                // Note: refresh_token is first parameter, access_token is second
                let oauth_tokens = if let Some(refresh_tok) = refresh_token {
                    OAuthTokens::new(refresh_tok, access_token, expires_at)
                } else {
                    // For providers without refresh token, use access token as refresh token
                    OAuthTokens::new(access_token.clone(), access_token, expires_at)
                };

                // Create credential with both OAuth token and API key
                let credential = ProviderCredential::new_oauth_with_api_key(
                    self.provider_id.clone(),
                    api_key,
                    oauth_tokens,
                );

                Ok(credential)
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "Expected OAuthTokens result for OAuth with API key flow".to_string(),
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

        // Use the stored access token to fetch fresh API key
        let (new_api_key, expires_at) = self
            .copilot_service
            .get_copilot_api_key(&oauth_tokens.access_token)
            .await
            .map_err(|e| {
                AuthFlowError::RefreshFailed(format!("Failed to refresh API key: {}", e))
            })?;

        // Create updated OAuth tokens with new expiry
        let updated_tokens = OAuthTokens::new(
            oauth_tokens.refresh_token.clone(),
            oauth_tokens.access_token.clone(),
            expires_at,
        );

        // Create new credential with refreshed API key
        let mut refreshed = credential.clone();
        refreshed.api_key = Some(new_api_key);
        refreshed.oauth_tokens = Some(updated_tokens);
        refreshed.updated_at = chrono::Utc::now();

        Ok(refreshed)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_oauth_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "test-client-id".to_string(),
            device_code_url: Some("https://github.com/login/device/code".to_string()),
            device_token_url: Some("https://github.com/login/oauth/access_token".to_string()),
            auth_url: None,
            token_url: None,
            scopes: vec!["read:user".to_string()],
            redirect_uri: String::new(),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        }
    }

    #[tokio::test]
    async fn test_auth_method_type() {
        let config = fixture_oauth_config();
        let oauth_service = Arc::new(ForgeOAuthService::new());
        let copilot_service = Arc::new(GitHubCopilotService::new());

        let flow = OAuthWithApiKeyFlow::new(
            ProviderId::GithubCopilot,
            config,
            oauth_service,
            copilot_service,
        );

        let actual = flow.auth_method_type();
        let expected = AuthMethodType::OAuthDevice;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_complete_with_wrong_result_type() {
        let config = fixture_oauth_config();
        let oauth_service = Arc::new(ForgeOAuthService::new());
        let copilot_service = Arc::new(GitHubCopilotService::new());

        let flow = OAuthWithApiKeyFlow::new(
            ProviderId::GithubCopilot,
            config,
            oauth_service,
            copilot_service,
        );

        let result =
            AuthResult::ApiKey { api_key: "test-key".to_string(), url_params: HashMap::new() };

        let actual = flow.complete(result).await;

        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Expected OAuthTokens")
        );
    }
}
