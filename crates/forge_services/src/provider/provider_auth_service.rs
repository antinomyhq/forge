//! Provider authentication service implementation
//!
//! Implements the `ProviderAuthService` trait using the auth flow factory
//! pattern. This service coordinates authentication flows for all provider
//! types including custom user-defined providers.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use forge_app::ProviderAuthService;
use forge_app::dto::{
    AuthContext, AuthInitiation, AuthResult, OAuthConfig, OAuthTokens, ProviderCredential,
    ProviderId,
};

use super::{AuthFlowError, AuthFlowInfra};
use crate::infra::{
    AppConfigRepository, EnvironmentInfra, ProviderCredentialRepository,
    ProviderSpecificProcessingInfra,
};
use crate::provider::AuthMethod;

/// Provider authentication service implementation
///
/// Coordinates authentication flows for LLM providers using the factory
/// pattern. Supports all authentication methods: API keys, OAuth device/code
/// flows, OAuth with API key exchange, cloud services, and custom providers.
pub struct ForgeProviderAuthService<I> {
    infra: Arc<I>,
}

impl<I> ForgeProviderAuthService<I> {
    /// Creates a new provider authentication service
    ///
    /// # Arguments
    /// * `infra` - Infrastructure providing OAuth, GitHub Copilot, and
    ///   credential repository
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    /// Helper to get URL parameters from provider config
    fn get_url_param_vars(&self, provider_id: &ProviderId) -> Vec<forge_app::dto::URLParam> {
        crate::provider::registry::get_provider_config(provider_id)
            .map(|config| config.url_param_vars.clone())
            .unwrap_or_default()
    }

    /// Handles API key authentication initiation
    async fn handle_api_key_init(
        &self,
        required_params: Vec<forge_app::dto::URLParam>,
    ) -> Result<AuthInitiation, super::AuthFlowError> {
        Ok(AuthInitiation::ApiKeyPrompt { required_params })
    }

    /// Handles API key authentication completion
    async fn handle_api_key_complete(
        &self,
        provider_id: ProviderId,
        api_key: String,
        url_params: std::collections::HashMap<String, String>,
    ) -> Result<ProviderCredential, super::AuthFlowError> {
        Ok(ProviderCredential::new_api_key(provider_id, api_key).url_params(url_params))
    }
}

impl<I> ForgeProviderAuthService<I>
where
    I: AuthFlowInfra,
{
    /// Handles OAuth device flow initiation
    async fn handle_oauth_device_init(
        &self,
        config: &crate::provider::OAuthConfig,
    ) -> Result<AuthInitiation, super::AuthFlowError> {
        // Validate configuration
        // Build oauth2 client
        use oauth2::basic::BasicClient;
        use oauth2::{ClientId, DeviceAuthorizationUrl, Scope, TokenUrl};

        use super::AuthFlowError;

        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(config.auth_url.clone()).map_err(|e| {
                    AuthFlowError::InitiationFailed(format!("Invalid auth_url: {}", e))
                })?,
            )
            .set_token_uri(TokenUrl::new(config.token_url.clone()).map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Invalid token_url: {}", e))
            })?);

        // Request device authorization
        let mut request = client.exchange_device_code();
        for scope in &config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }

        // Build HTTP client with custom headers
        let oauth_service = self.infra.oauth_service();
        let http_client = oauth_service
            .build_http_client(config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        let http_fn = |req| {
            crate::provider::ForgeOAuthService::github_compliant_http_request(
                http_client.clone(),
                req,
            )
        };

        let device_auth_response: oauth2::StandardDeviceAuthorizationResponse =
            request.request_async(&http_fn).await.map_err(|e| {
                AuthFlowError::InitiationFailed(format!(
                    "Device authorization request failed: {}",
                    e
                ))
            })?;

        // Build context with device code and interval for polling
        let context = AuthContext::device(
            device_auth_response.device_code().secret().to_string(),
            device_auth_response.interval().as_secs(),
        );

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

    /// Handles OAuth device flow polling until completion
    ///
    /// # Errors
    /// Returns error if polling fails, times out, or auth is denied
    async fn handle_oauth_device_poll(
        &self,
        device_code: &str,
        _interval: u64,
        config: &crate::provider::OAuthConfig,
        timeout: Duration,
    ) -> Result<AuthResult, super::AuthFlowError> {
        // Build oauth2 client for polling
        use oauth2::basic::BasicClient;
        use oauth2::{ClientId, DeviceAuthorizationUrl, DeviceCode, TokenUrl};

        use super::AuthFlowError;

        let auth_url = &config.auth_url;
        let token_url = &config.token_url;

        let _client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(auth_url.clone())
                    .map_err(|e| AuthFlowError::PollFailed(format!("Invalid auth_url: {}", e)))?,
            )
            .set_token_uri(
                TokenUrl::new(token_url.clone())
                    .map_err(|e| AuthFlowError::PollFailed(format!("Invalid token_url: {}", e)))?,
            );

        // Build HTTP client for manual polling
        let oauth_service = self.infra.oauth_service();
        let http_client = oauth_service
            .build_http_client(config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        // Create a device code wrapper
        let device_code = DeviceCode::new(device_code.to_string());

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
                ("client_id".to_string(), config.client_id.clone()),
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
            if let Some(custom_headers) = &config.custom_headers {
                for (key, value) in custom_headers {
                    if let (Ok(name), Ok(val)) =
                        (HeaderName::try_from(key), HeaderValue::from_str(value))
                    {
                        headers.insert(name, val);
                    }
                }
            }

            let response = http_client
                .post(token_url)
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

    /// Handles OAuth device flow completion
    ///
    /// # Errors
    /// Returns error if result type is invalid or credential creation fails
    async fn handle_oauth_device_complete(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
    ) -> Result<ProviderCredential, super::AuthFlowError> {
        use super::AuthFlowError;

        match result {
            AuthResult::OAuthTokens { access_token, refresh_token, expires_in } => {
                use chrono::Utc;

                // Calculate expiration time
                let expires_at = if let Some(seconds) = expires_in {
                    Utc::now() + chrono::Duration::seconds(seconds as i64)
                } else {
                    // Default to 1 year if not specified
                    Utc::now() + chrono::Duration::days(365)
                };

                let oauth_tokens = forge_app::dto::OAuthTokens::new(
                    refresh_token.unwrap_or_else(|| access_token.clone()),
                    access_token,
                    expires_at,
                );

                Ok(ProviderCredential::new_oauth(provider_id, oauth_tokens))
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "Expected OAuth tokens result".to_string(),
            )),
        }
    }

    /// Handles OAuth authorization code flow initiation
    ///
    /// # Errors
    /// Returns error if authorization URL generation fails
    async fn handle_oauth_code_init(
        &self,
        _provider_id: &ProviderId,
        config: &crate::provider::OAuthConfig,
    ) -> Result<AuthInitiation, super::AuthFlowError> {
        use super::AuthFlowError;

        // Build authorization URL with PKCE
        let oauth_service = self.infra.oauth_service();
        let auth_params = oauth_service.build_auth_code_url(config).map_err(|e| {
            AuthFlowError::InitiationFailed(format!("Failed to build auth URL: {}", e))
        })?;

        // Build context with state and PKCE verifier
        let context =
            AuthContext::code(auth_params.state.clone(), auth_params.code_verifier.clone());

        Ok(AuthInitiation::CodeFlow {
            authorization_url: auth_params.auth_url,
            state: auth_params.state,
            context,
        })
    }

    /// Handles OAuth authorization code flow completion
    ///
    /// # Errors
    /// Returns error if code exchange fails or credential creation fails
    async fn handle_oauth_code_complete(
        &self,
        provider_id: ProviderId,
        code: String,
        code_verifier: Option<String>,
        config: &crate::provider::OAuthConfig,
    ) -> Result<ProviderCredential, super::AuthFlowError> {
        use super::AuthFlowError;

        // Exchange code for tokens with PKCE verifier (if provided)
        let oauth_service = self.infra.oauth_service();
        let token_response = oauth_service
            .exchange_auth_code(config, &code, code_verifier.as_deref())
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
            forge_app::dto::OAuthTokens::new(refresh_token, token_response.access_token, expires_at)
        } else {
            // For providers without refresh token, use access token as refresh token
            forge_app::dto::OAuthTokens::new(
                token_response.access_token.clone(),
                token_response.access_token,
                expires_at,
            )
        };

        // Create credential
        Ok(ProviderCredential::new_oauth(provider_id, oauth_tokens))
    }

    /// Handles OAuth device flow with API key exchange (GitHub Copilot
    /// pattern).
    ///
    /// This initiates the OAuth device flow that will later exchange tokens for
    /// API keys. Returns device code and verification URL for user
    /// authorization.
    async fn handle_oauth_with_apikey_init(
        &self,
        _provider_id: &ProviderId,
        config: &crate::provider::OAuthConfig,
    ) -> Result<AuthInitiation, super::AuthFlowError> {
        use super::AuthFlowError;

        // Validate configuration
        // Build oauth2 client
        use oauth2::basic::BasicClient;
        use oauth2::{ClientId, DeviceAuthorizationUrl, Scope, TokenUrl};

        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new((&config.auth_url).clone()).map_err(|e| {
                    AuthFlowError::InitiationFailed(format!("Invalid auth_url: {}", e))
                })?,
            )
            .set_token_uri(TokenUrl::new((&config.token_url).clone()).map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Invalid token_url: {}", e))
            })?);

        // Request device authorization with scopes
        let mut request = client.exchange_device_code();
        for scope in &config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }

        // Build HTTP client with custom headers
        let http_client = self
            .infra
            .oauth_service()
            .build_http_client(config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        let http_fn = |req| {
            crate::provider::oauth::ForgeOAuthService::github_compliant_http_request(
                http_client.clone(),
                req,
            )
        };

        let device_auth_response: oauth2::StandardDeviceAuthorizationResponse =
            request.request_async(&http_fn).await.map_err(|e| {
                AuthFlowError::InitiationFailed(format!(
                    "Device authorization request failed: {}",
                    e
                ))
            })?;

        // Build context with device code and interval for polling
        let interval = device_auth_response.interval().as_secs();
        let context = AuthContext::device(
            device_auth_response.device_code().secret().to_string(),
            interval,
        );

        Ok(AuthInitiation::DeviceFlow {
            user_code: device_auth_response.user_code().secret().to_string(),
            verification_uri: device_auth_response.verification_uri().to_string(),
            verification_uri_complete: device_auth_response
                .verification_uri_complete()
                .map(|u| u.secret().to_string()),
            expires_in: device_auth_response.expires_in().as_secs(),
            interval,
            context,
        })
    }

    /// Polls for OAuth tokens using device code (GitHub Copilot pattern).
    ///
    /// This implements manual polling with GitHub-specific response handling.
    async fn handle_oauth_with_apikey_poll(
        &self,
        device_code: &str,
        config: &crate::provider::OAuthConfig,
        timeout: Duration,
    ) -> Result<AuthResult, super::AuthFlowError> {
        use super::AuthFlowError;

        let token_url = &config.token_url;

        // Build HTTP client for manual polling
        let http_client = self
            .infra
            .oauth_service()
            .build_http_client(config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

        let start_time = tokio::time::Instant::now();
        let interval = Duration::from_secs(5);

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
                ("device_code".to_string(), device_code.to_string()),
                ("client_id".to_string(), config.client_id.clone()),
            ];

            let body = serde_urlencoded::to_string(&params).map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to encode request: {}", e))
            })?;

            // Make HTTP request with headers
            let mut headers = HeaderMap::new();
            headers.insert(
                "Content-Type",
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
            headers.insert("Accept", HeaderValue::from_static("application/json"));

            // Add custom headers
            if let Some(custom_headers) = &config.custom_headers {
                for (key, value) in custom_headers {
                    if let (Ok(name), Ok(val)) =
                        (HeaderName::try_from(key), HeaderValue::from_str(value))
                    {
                        headers.insert(name, val);
                    }
                }
            }

            let response = http_client
                .post(token_url)
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
                            // Still waiting - continue polling
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
                    // User hasn't authorized yet
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

    /// Completes OAuth with API key flow by exchanging OAuth token for API key.
    ///
    /// This uses the GitHub Copilot service to convert the OAuth access token
    /// into a time-limited API key. Both are stored in the credential for
    /// refresh.
    async fn handle_oauth_with_apikey_complete(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
    ) -> Result<ProviderCredential, super::AuthFlowError> {
        use super::AuthFlowError;

        match result {
            AuthResult::OAuthTokens { access_token, refresh_token, expires_in: _ } => {
                // Exchange OAuth token for API key
                let (api_key, expires_at) = self
                    .infra
                    .github_copilot_service()
                    .get_copilot_api_key(&access_token)
                    .await
                    .map_err(|e| {
                        AuthFlowError::CompletionFailed(format!(
                            "Failed to exchange OAuth token for API key: {}",
                            e
                        ))
                    })?;

                // Create OAuth tokens structure
                let oauth_tokens = if let Some(refresh_tok) = refresh_token {
                    OAuthTokens::new(refresh_tok, access_token, expires_at)
                } else {
                    // Use access token as refresh token if none provided
                    OAuthTokens::new(access_token.clone(), access_token, expires_at)
                };

                // Create credential with both OAuth token and API key
                let credential =
                    ProviderCredential::new_oauth_with_api_key(provider_id, api_key, oauth_tokens);

                Ok(credential)
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "Expected OAuthTokens result for OAuth with API key flow".to_string(),
            )),
        }
    }

    // ========== Refresh Handlers ==========

    /// Handles API key credential refresh (not supported)
    fn handle_api_key_refresh(
        &self,
        _credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        // Static API keys don't support refresh
        Err(AuthFlowError::RefreshFailed(
            "API key credentials cannot be refreshed".to_string(),
        ))
    }

    /// Handles OAuth device flow credential refresh
    async fn handle_oauth_device_refresh(
        &self,
        credential: &ProviderCredential,
        config: &OAuthConfig,
    ) -> Result<ProviderCredential, AuthFlowError> {
        let tokens = credential.oauth_tokens.as_ref().ok_or_else(|| {
            AuthFlowError::RefreshFailed("No OAuth tokens in credential".to_string())
        })?;

        // Use OAuth service to refresh token
        let token_response = self
            .infra
            .oauth_service()
            .refresh_access_token(config, &tokens.refresh_token)
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

    /// Handles OAuth code flow credential refresh
    async fn handle_oauth_code_refresh(
        &self,
        credential: &ProviderCredential,
        config: &OAuthConfig,
    ) -> Result<ProviderCredential, AuthFlowError> {
        // Get stored OAuth tokens
        let oauth_tokens = credential.oauth_tokens.as_ref().ok_or_else(|| {
            AuthFlowError::RefreshFailed("Missing OAuth tokens in credential".to_string())
        })?;

        // Use refresh token to get new access token
        let token_response = self
            .infra
            .oauth_service()
            .refresh_access_token(config, &oauth_tokens.refresh_token)
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

    /// Handles OAuth with API key credential refresh (GitHub Copilot pattern)
    async fn handle_oauth_with_apikey_refresh(
        &self,
        credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        // Get stored OAuth tokens
        let oauth_tokens = credential.oauth_tokens.as_ref().ok_or_else(|| {
            AuthFlowError::RefreshFailed("Missing OAuth tokens in credential".to_string())
        })?;

        // Use the stored access token to fetch fresh API key
        let (new_api_key, expires_at) = self
            .infra
            .github_copilot_service()
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

#[async_trait::async_trait]
impl<I> ProviderAuthService for ForgeProviderAuthService<I>
where
    I: AuthFlowInfra
        + ProviderCredentialRepository
        + EnvironmentInfra
        + AppConfigRepository
        + ProviderSpecificProcessingInfra
        + Send
        + Sync
        + 'static,
{
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        method: AuthMethod,
    ) -> anyhow::Result<AuthInitiation> {
        // Get URL parameters from provider config
        let url_param_vars = self.get_url_param_vars(&provider_id);

        // Dispatch based on auth method
        match &method {
            AuthMethod::ApiKey => {
                // Handle API key auth directly
                self.handle_api_key_init(url_param_vars)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))
            }
            AuthMethod::OAuthDevice(config) => {
                // Check if this needs OAuth with API key exchange (GitHub Copilot pattern)
                if config.token_refresh_url.is_some() {
                    // Handle OAuth with API key directly
                    self.handle_oauth_with_apikey_init(&provider_id, config)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                } else {
                    // Handle OAuth device flow directly
                    self.handle_oauth_device_init(config)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                }
            }
            AuthMethod::OAuthCode(config) => {
                // Handle OAuth code flow directly
                self.handle_oauth_code_init(&provider_id, config)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))
            }
        }
    }

    async fn poll_provider_auth(
        &self,
        _provider_id: ProviderId,
        context: &AuthContext,
        timeout: Duration,
        method: AuthMethod,
    ) -> anyhow::Result<AuthResult> {
        // Dispatch based on auth method
        match &method {
            AuthMethod::ApiKey => {
                // API key flows are not pollable
                Err(anyhow::anyhow!(
                    "API key authentication requires manual input and cannot be polled"
                ))
            }
            AuthMethod::OAuthDevice(config) => {
                // Extract device code from context
                let (device_code, interval) = match context {
                    AuthContext::Device { device_code, interval } => {
                        (device_code.as_str(), *interval)
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Invalid context type for device flow"));
                    }
                };

                // Check if this needs OAuth with API key exchange (GitHub Copilot pattern)
                if config.token_refresh_url.is_some() {
                    // Handle OAuth with API key polling directly
                    self.handle_oauth_with_apikey_poll(device_code, config, timeout)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                } else {
                    // Handle OAuth device flow polling directly
                    self.handle_oauth_device_poll(device_code, interval, config, timeout)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                }
            }
            AuthMethod::OAuthCode(_config) => {
                // OAuth code flow requires manual code entry, no polling
                Err(anyhow::anyhow!(
                    "OAuth code flow requires manual authorization code entry"
                ))
            }
        }
    }

    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
        method: AuthMethod,
    ) -> anyhow::Result<ProviderCredential> {
        // Dispatch based on auth method and result type
        let credential = match (&method, &result) {
            (AuthMethod::ApiKey, AuthResult::ApiKey { api_key, url_params }) => {
                // Handle API key auth directly
                self.handle_api_key_complete(
                    provider_id.clone(),
                    api_key.clone(),
                    url_params.clone(),
                )
                .await
                .map_err(|e| anyhow::anyhow!(e))?
            }
            (AuthMethod::OAuthDevice(config), AuthResult::OAuthTokens { .. }) => {
                // Check if this needs OAuth with API key exchange (GitHub Copilot pattern)
                if config.token_refresh_url.is_some() {
                    // Handle OAuth with API key completion directly
                    self.handle_oauth_with_apikey_complete(provider_id.clone(), result)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?
                } else {
                    // Handle OAuth device flow completion directly
                    self.handle_oauth_device_complete(provider_id.clone(), result)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?
                }
            }
            (
                AuthMethod::OAuthCode(config),
                AuthResult::AuthorizationCode { code, code_verifier, .. },
            ) => {
                // Handle OAuth code flow completion directly
                self.handle_oauth_code_complete(
                    provider_id.clone(),
                    code.clone(),
                    code_verifier.clone(),
                    config,
                )
                .await
                .map_err(|e| anyhow::anyhow!(e))?
            }
            _ => {
                // Unknown combination
                return Err(anyhow::anyhow!(
                    "Unsupported auth method or result type combination"
                ));
            }
        };

        // Store credential via infrastructure (takes ownership)
        self.infra.upsert_credential(credential.clone()).await?;
        Ok(credential)
    }

    async fn refresh_provider_credential(
        &self,
        _provider_id: ProviderId,
        credential: &ProviderCredential,
        method: AuthMethod,
    ) -> anyhow::Result<ProviderCredential> {
        // Dispatch to appropriate refresh handler based on auth method
        let refreshed_credential = match &method {
            AuthMethod::ApiKey => {
                // API key doesn't support refresh
                self.handle_api_key_refresh(credential)
                    .map_err(|e| anyhow::anyhow!(e))?
            }
            AuthMethod::OAuthDevice(config) => {
                // Check if this is OAuth with API key (GitHub Copilot pattern)
                if config.token_refresh_url.is_some() {
                    // OAuth with API key refresh
                    self.handle_oauth_with_apikey_refresh(credential)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?
                } else {
                    // Standard OAuth device flow refresh
                    self.handle_oauth_device_refresh(credential, config)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?
                }
            }
            AuthMethod::OAuthCode(config) => {
                // OAuth code flow refresh
                self.handle_oauth_code_refresh(credential, config)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?
            }
        };

        // Update credential in database
        self.infra
            .upsert_credential(refreshed_credential.clone())
            .await?;

        Ok(refreshed_credential)
    }
}
