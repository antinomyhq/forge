/// OAuth Device Flow authentication for generic OAuth providers
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use forge_app::dto::{
    AuthContext, AuthInitiation, AuthResult, OAuthTokens, ProviderCredential,
    ProviderId,
};

use super::{AuthFlowError, AuthenticationFlow};
use crate::provider::{ForgeOAuthService, OAuthConfig};

/// OAuth Device Flow authentication.
///
/// Used by generic OAuth providers that support the device authorization flow:
/// 1. Request device code → Get user_code and verification_uri
/// 2. Display code and URL to user
/// 3. Poll token endpoint until user authorizes
/// 4. Return OAuth tokens
///
/// This is the standard OAuth Device Flow (RFC 8628).
pub struct OAuthDeviceFlow {
    provider_id: ProviderId,
    config: OAuthConfig,
    oauth_service: Arc<ForgeOAuthService>,
}

impl OAuthDeviceFlow {
    /// Creates a new OAuth Device Flow authenticator.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - The provider requiring authentication
    /// * `config` - OAuth configuration (device_code_url, device_token_url,
    ///   client_id, scopes)
    /// * `oauth_service` - OAuth service for making HTTP requests
    pub fn new(
        provider_id: ProviderId,
        config: OAuthConfig,
        oauth_service: Arc<ForgeOAuthService>,
    ) -> Self {
        Self { provider_id, config, oauth_service }
    }
}

#[async_trait::async_trait]
impl AuthenticationFlow for OAuthDeviceFlow {

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

        // Build oauth2 client for polling
        use oauth2::basic::BasicClient;
        use oauth2::{ClientId, DeviceAuthorizationUrl, DeviceCode, TokenUrl};

        let device_code_url = self.config.device_code_url.as_ref().ok_or_else(|| {
            AuthFlowError::PollFailed("device_code_url not configured".to_string())
        })?;
        let device_token_url = self.config.device_token_url.as_ref().ok_or_else(|| {
            AuthFlowError::PollFailed("device_token_url not configured".to_string())
        })?;

        let _client = BasicClient::new(ClientId::new(self.config.client_id.clone()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(device_code_url.clone()).map_err(|e| {
                    AuthFlowError::PollFailed(format!("Invalid device_code_url: {}", e))
                })?,
            )
            .set_token_uri(TokenUrl::new(device_token_url.clone()).map_err(|e| {
                AuthFlowError::PollFailed(format!("Invalid device_token_url: {}", e))
            })?);

        // Build HTTP client for manual polling
        // We manually implement polling (option 3) for better control and timeout
        // handling
        let http_client = self
            .oauth_service
            .build_http_client(self.config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        // Create a device code wrapper
        let device_code = DeviceCode::new(device_code.clone());

        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

        let start_time = tokio::time::Instant::now();
        let interval = Duration::from_secs(5); // Default interval

        loop {
            // Check timeout
            if start_time.elapsed() >= timeout {
                return Err(AuthFlowError::Timeout(timeout));
            }

            // Build token request
            let params = vec![
                (
                    "grant_type".to_string(),
                    "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                ),
                ("device_code".to_string(), device_code.secret().to_string()),
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
            let body = response.text().await.map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to read response: {}", e))
            })?;

            // Parse response
            if status.is_success() {
                // Success - parse token response
                let token_response: serde_json::Value =
                    serde_json::from_str(&body).map_err(|e| {
                        AuthFlowError::PollFailed(format!("Failed to parse token response: {}", e))
                    })?;

                let access_token = token_response["access_token"]
                    .as_str()
                    .ok_or_else(|| {
                        AuthFlowError::PollFailed("Missing access_token in response".to_string())
                    })?
                    .to_string();

                let refresh_token = token_response["refresh_token"]
                    .as_str()
                    .map(|s| s.to_string());
                let expires_in = token_response["expires_in"].as_u64();

                return Ok(AuthResult::OAuthTokens { access_token, refresh_token, expires_in });
            } else {
                // Check for error response
                if let Ok(error_response) = serde_json::from_str::<serde_json::Value>(&body)
                    && let Some(error) = error_response["error"].as_str()
                {
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

                // Unknown error
                return Err(AuthFlowError::PollFailed(format!(
                    "HTTP {}: {}",
                    status, body
                )));
            }
        }
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match result {
            AuthResult::OAuthTokens { access_token, refresh_token, expires_in } => {
                // Calculate expiration time
                let expires_at = if let Some(seconds) = expires_in {
                    Utc::now() + chrono::Duration::seconds(seconds as i64)
                } else {
                    // Default to 1 year if not specified
                    Utc::now() + chrono::Duration::days(365)
                };

                let oauth_tokens = OAuthTokens::new(
                    refresh_token.unwrap_or_else(|| access_token.clone()),
                    access_token,
                    expires_at,
                );

                Ok(ProviderCredential::new_oauth(
                    self.provider_id.clone(),
                    oauth_tokens,
                ))
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "Expected OAuth tokens result".to_string(),
            )),
        }
    }

    async fn refresh(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        let tokens = credential.oauth_tokens.as_ref().ok_or_else(|| {
            AuthFlowError::RefreshFailed("No OAuth tokens in credential".to_string())
        })?;

        // Use OAuth service to refresh token
        let token_response = self
            .oauth_service
            .refresh_access_token(&self.config, &tokens.refresh_token)
            .await
            .map_err(|e| AuthFlowError::RefreshFailed(format!("Token refresh failed: {}", e)))?;

        // Calculate new expiration
        let expires_at = if let Some(seconds) = token_response.expires_in {
            Utc::now() + chrono::Duration::seconds(seconds as i64)
        } else {
            Utc::now() + chrono::Duration::days(365)
        };

        let new_tokens = OAuthTokens::new(
            token_response
                .refresh_token
                .unwrap_or_else(|| tokens.refresh_token.clone()),
            token_response.access_token,
            expires_at,
        );

        let mut updated_credential = credential.clone();
        updated_credential.update_oauth_tokens(new_tokens);

        Ok(updated_credential)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_config() -> OAuthConfig {
        OAuthConfig {
            device_code_url: Some("https://github.com/login/device/code".to_string()),
            device_token_url: Some("https://github.com/login/oauth/access_token".to_string()),
            auth_url: None,
            token_url: None,
            client_id: "Iv1.test123".to_string(),
            scopes: vec!["read:user".to_string()],
            redirect_uri: String::new(),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        }
    }

    fn create_flow() -> OAuthDeviceFlow {
        OAuthDeviceFlow::new(
            ProviderId::GithubCopilot,
            create_config(),
            Arc::new(ForgeOAuthService::new()),
        )
    }


    // Note: Full integration tests for initiate() and poll_until_complete()
    // would require mocking the OAuth server, which is beyond the scope of unit
    // tests. These should be tested in integration tests with a mock server.

    #[tokio::test]
    async fn test_complete_success() {
        let flow = create_flow();
        let result = AuthResult::OAuthTokens {
            access_token: "gho_test123".to_string(),
            refresh_token: Some("refresh_test456".to_string()),
            expires_in: Some(3600),
        };

        let credential = flow.complete(result).await.unwrap();

        assert_eq!(credential.provider_id, ProviderId::GithubCopilot);
        assert_eq!(credential.get_access_token(), Some("gho_test123"));
        assert!(credential.oauth_tokens.is_some());
    }

    #[tokio::test]
    async fn test_complete_without_refresh_token() {
        let flow = create_flow();
        let result = AuthResult::OAuthTokens {
            access_token: "gho_test123".to_string(),
            refresh_token: None,
            expires_in: Some(3600),
        };

        let credential = flow.complete(result).await.unwrap();

        // Should use access token as refresh token fallback
        let tokens = credential.oauth_tokens.unwrap();
        assert_eq!(tokens.access_token, "gho_test123");
        assert_eq!(tokens.refresh_token, "gho_test123");
    }

    #[tokio::test]
    async fn test_complete_wrong_result_type() {
        let flow = create_flow();
        let result =
            AuthResult::ApiKey { api_key: "sk-test".to_string(), url_params: HashMap::new() };

        let error = flow.complete(result).await.unwrap_err();
        assert!(matches!(error, AuthFlowError::CompletionFailed(_)));
    }
}
