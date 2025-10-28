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
    AccessToken, ApiKey, AuthContext, AuthResult, AuthorizationCode, OAuthConfig, OAuthTokens,
    PkceVerifier, ProviderCredential, ProviderId, RefreshToken, URLParam, URLParamValue,
};

use super::AuthFlowError;
use crate::infra::{AppConfigRepository, EnvironmentInfra, ProviderCredentialRepository};
use crate::provider::{AuthMethod, ForgeOAuthService, OAuthTokenResponse};
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
    /// * `infra` - Infrastructure for credential repository and environment
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
    ) -> Result<forge_app::dto::AuthContext, super::AuthFlowError> {
        use forge_app::dto::{ApiKeyMethod, ApiKeyRequest, ApiKeyResponse, AuthContext};

        Ok(AuthContext::api_key(
            ApiKeyRequest { required_params },
            ApiKeyResponse {
                api_key: String::new().into(),
                url_params: std::collections::HashMap::new(),
            },
            ApiKeyMethod,
        ))
    }

    /// Handles API key authentication completion
    async fn handle_api_key_complete(
        &self,
        provider_id: ProviderId,
        api_key: ApiKey,
        url_params: std::collections::HashMap<URLParam, URLParamValue>,
    ) -> Result<ProviderCredential, super::AuthFlowError> {
        Ok(ProviderCredential::new_api_key(provider_id, api_key).url_params(url_params))
    }
    /// Injects custom headers into a HeaderMap
    fn inject_custom_headers(
        headers: &mut reqwest::header::HeaderMap,
        custom_headers: &Option<std::collections::HashMap<String, String>>,
    ) {
        use reqwest::header::{HeaderName, HeaderValue};

        if let Some(custom_headers) = custom_headers {
            for (key, value) in custom_headers {
                if let (Ok(name), Ok(val)) =
                    (HeaderName::try_from(key), HeaderValue::from_str(value))
                {
                    headers.insert(name, val);
                }
            }
        }
    }

    /// Parses and handles OAuth error responses during polling
    fn handle_oauth_error(error_code: &str) -> Result<(), AuthFlowError> {
        match error_code {
            "authorization_pending" | "slow_down" => Ok(()),
            "expired_token" => Err(AuthFlowError::Expired),
            "access_denied" => Err(AuthFlowError::Denied),
            _ => Err(AuthFlowError::PollFailed(format!(
                "OAuth error: {}",
                error_code
            ))),
        }
    }

    /// Parses token response from JSON
    ///
    /// Extracts access_token, refresh_token, and expires_in from OAuth
    /// response.
    ///
    /// # Arguments
    /// * `body` - JSON response body as string
    ///
    /// # Returns
    /// Tuple of (access_token, refresh_token, expires_in)
    ///
    /// # Errors
    /// Returns `AuthFlowError::PollFailed` if parsing fails or access_token is
    /// missing
    fn parse_token_response(
        body: &str,
    ) -> Result<(String, Option<String>, Option<u64>), AuthFlowError> {
        let token_response: serde_json::Value = serde_json::from_str(body).map_err(|e| {
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

        Ok((access_token, refresh_token, expires_in))
    }

    /// Calculates token expiration time
    fn calculate_token_expiry(
        expires_in: Option<u64>,
        fallback: chrono::Duration,
    ) -> chrono::DateTime<chrono::Utc> {
        if let Some(seconds) = expires_in {
            Utc::now() + chrono::Duration::seconds(seconds as i64)
        } else {
            Utc::now() + fallback
        }
    }

    /// Builds a provider credential from OAuth tokens
    fn build_oauth_credential(
        provider_id: ProviderId,
        access_token: impl Into<AccessToken>,
        refresh_token: Option<impl Into<RefreshToken>>,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> ProviderCredential {
        ProviderCredential::new_oauth(
            provider_id,
            OAuthTokens {
                access_token: access_token.into(),
                refresh_token: refresh_token
                    .map(Into::into)
                    .unwrap_or_else(|| String::new().into()),
                expires_at,
            },
        )
    }
}

impl<I> ForgeProviderAuthService<I> {
    /// Handles OAuth device flow initiation
    async fn handle_oauth_device_init(
        &self,
        config: &crate::provider::OAuthConfig,
    ) -> Result<forge_app::dto::AuthContext, super::AuthFlowError> {
        // Validate configuration
        // Build oauth2 client
        use oauth2::basic::BasicClient;
        use oauth2::{ClientId, DeviceAuthorizationUrl, Scope, TokenUrl};

        use super::AuthFlowError;

        let client = BasicClient::new(ClientId::new(config.client_id.to_string()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(config.auth_url.to_string()).map_err(|e| {
                    AuthFlowError::InitiationFailed(format!("Invalid auth_url: {}", e))
                })?,
            )
            .set_token_uri(TokenUrl::new(config.token_url.to_string()).map_err(|e| {
                AuthFlowError::InitiationFailed(format!("Invalid token_url: {}", e))
            })?);

        // Request device authorization
        let mut request = client.exchange_device_code();
        for scope in &config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }

        // Build HTTP client with custom headers
        let http_client = ForgeOAuthService::build_http_client(config.custom_headers.as_ref())
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

        use forge_app::dto::{
            AuthContext, DeviceCodeMethod, DeviceCodeRequest, DeviceCodeResponse,
        };

        // Build the type-safe context
        Ok(AuthContext::device_code(
            DeviceCodeRequest {
                user_code: device_auth_response.user_code().secret().to_string().into(),
                verification_uri: device_auth_response.verification_uri().to_string().into(),
                verification_uri_complete: device_auth_response
                    .verification_uri_complete()
                    .map(|u| u.secret().to_string().into()),
                expires_in: device_auth_response.expires_in().as_secs(),
                interval: device_auth_response.interval().as_secs(),
            },
            DeviceCodeResponse {
                device_code: device_auth_response
                    .device_code()
                    .secret()
                    .to_string()
                    .into(),
                interval: device_auth_response.interval().as_secs(),
            },
            DeviceCodeMethod { oauth_config: config.clone() },
        ))
    }

    /// Handles OAuth device flow polling until completion
    ///
    /// # Errors
    /// Returns error if polling fails, times out, or auth is denied
    async fn handle_oauth_device_poll(
        &self,
        device_code: &str,
        config: &crate::provider::OAuthConfig,
        timeout: Duration,
        github_compatible: bool,
    ) -> Result<AuthResult, super::AuthFlowError> {
        use super::AuthFlowError;

        // Build HTTP client for manual polling
        let http_client = ForgeOAuthService::build_http_client(config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        use reqwest::header::{HeaderMap, HeaderValue};

        let start_time = tokio::time::Instant::now();
        let interval = Duration::from_secs(5);

        loop {
            // Check timeout
            if start_time.elapsed() >= timeout {
                return Err(AuthFlowError::Timeout(timeout));
            }

            // Sleep before polling (GitHub pattern only)
            if github_compatible {
                tokio::time::sleep(interval).await;
            }

            // Build token request
            let params = vec![
                (
                    "grant_type".to_string(),
                    "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                ),
                ("device_code".to_string(), device_code.to_string()),
                ("client_id".to_string(), config.client_id.to_string()),
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

            // Inject custom headers using helper
            Self::inject_custom_headers(&mut headers, &config.custom_headers);

            let response = http_client
                .post(config.token_url.as_str())
                .headers(headers)
                .body(body)
                .send()
                .await
                .map_err(|e| AuthFlowError::PollFailed(format!("HTTP request failed: {}", e)))?;

            let status = response.status();
            let body_text = response.text().await.map_err(|e| {
                AuthFlowError::PollFailed(format!("Failed to read response: {}", e))
            })?;

            // GitHub-compatible: HTTP 200 can contain either success or error
            if github_compatible && status.is_success() {
                let token_response: serde_json::Value = serde_json::from_str(&body_text)
                    .unwrap_or_else(|_| serde_json::json!({"error": "parse_error"}));

                // Check for error field first
                if let Some(error) = token_response["error"].as_str() {
                    if Self::handle_oauth_error(error).is_ok() {
                        // Retryable error - continue polling (already slept before request)
                        continue;
                    }
                    // Terminal error - propagate
                    return Err(Self::handle_oauth_error(error).unwrap_err());
                }

                // No error field - parse as success
                let (access_token, refresh_token, expires_in) =
                    Self::parse_token_response(&body_text)?;

                return Ok(AuthResult::OAuthTokens {
                    access_token: access_token.into(),
                    refresh_token: refresh_token.map(Into::into),
                    expires_in,
                });
            }

            // Standard OAuth: HTTP success means tokens
            if !github_compatible && status.is_success() {
                let (access_token, refresh_token, expires_in) =
                    Self::parse_token_response(&body_text)?;

                return Ok(AuthResult::OAuthTokens {
                    access_token: access_token.into(),
                    refresh_token: refresh_token.map(Into::into),
                    expires_in,
                });
            }

            // Handle error responses (non-200 status for standard OAuth)
            let error_response: serde_json::Value = serde_json::from_str(&body_text)
                .unwrap_or_else(|_| serde_json::json!({"error": "unknown_error"}));

            if let Some(error) = error_response["error"].as_str() {
                if Self::handle_oauth_error(error).is_ok() {
                    // Retryable error - sleep and continue
                    tokio::time::sleep(if error == "slow_down" {
                        interval * 2
                    } else {
                        interval
                    })
                    .await;
                    continue;
                }
                // Terminal error - propagate
                return Err(Self::handle_oauth_error(error).unwrap_err());
            }

            // Unknown error
            return Err(AuthFlowError::PollFailed(format!(
                "HTTP {}: {}",
                status, body_text
            )));
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
                // Use helpers for consistent credential building
                let expires_at =
                    Self::calculate_token_expiry(expires_in, chrono::Duration::days(365));

                Ok(Self::build_oauth_credential(
                    provider_id,
                    access_token,
                    refresh_token,
                    expires_at,
                ))
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
    ) -> Result<forge_app::dto::AuthContext, super::AuthFlowError> {
        use super::AuthFlowError;

        // Build authorization URL with PKCE
        let auth_params = ForgeOAuthService::build_auth_code_url(config).map_err(|e| {
            AuthFlowError::InitiationFailed(format!("Failed to build auth URL: {}", e))
        })?;

        use forge_app::dto::{AuthContext, CodeMethod, CodeRequest, CodeResponse};

        // Build the type-safe context
        Ok(AuthContext::code(
            CodeRequest {
                authorization_url: auth_params.auth_url.into(),
                state: auth_params.state.clone().into(),
            },
            CodeResponse {
                state: auth_params.state.into(),
                pkce_verifier: auth_params.code_verifier.map(Into::into),
            },
            CodeMethod { oauth_config: config.clone() },
        ))
    }

    /// Handles OAuth authorization code flow completion
    ///
    /// # Errors
    /// Returns error if code exchange fails or credential creation fails
    async fn handle_oauth_code_complete(
        &self,
        provider_id: ProviderId,
        code: AuthorizationCode,
        code_verifier: Option<PkceVerifier>,
        config: &crate::provider::OAuthConfig,
    ) -> Result<ProviderCredential, super::AuthFlowError> {
        use super::AuthFlowError;

        // Exchange code for tokens with PKCE verifier (if provided)
        let token_response = ForgeOAuthService::exchange_auth_code(
            config,
            code.as_str(),
            code_verifier.as_ref().map(|v| v.as_str()),
        )
        .await
        .map_err(|e| {
            AuthFlowError::CompletionFailed(format!("Failed to exchange authorization code: {}", e))
        })?;

        // Use helpers for consistent credential building
        let expires_at =
            Self::calculate_token_expiry(token_response.expires_in, chrono::Duration::hours(1));

        Ok(Self::build_oauth_credential(
            provider_id,
            token_response.access_token,
            token_response.refresh_token,
            expires_at,
        ))
    }

    /// Exchanges OAuth access token for API key (GitHub Copilot pattern).
    /// This fetches a time-limited API key from the token refresh URL using the
    /// OAuth access token.
    async fn exchange_oauth_for_api_key(
        &self,
        oauth_token: &str,
        config: &crate::provider::OAuthConfig,
    ) -> Result<(ApiKey, chrono::DateTime<chrono::Utc>), super::AuthFlowError> {
        use super::AuthFlowError;

        let token_refresh_url = config.token_refresh_url.as_ref().ok_or_else(|| {
            AuthFlowError::CompletionFailed("Missing token_refresh_url in config".to_string())
        })?;

        // Build request headers
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", oauth_token)).map_err(
                |e| AuthFlowError::CompletionFailed(format!("Invalid authorization header: {}", e)),
            )?,
        );

        // Add custom headers from config
        if let Some(custom_headers) = &config.custom_headers {
            for (key, value) in custom_headers {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::try_from(key),
                    reqwest::header::HeaderValue::from_str(value),
                ) {
                    headers.insert(name, val);
                }
            }
        }

        let response = ForgeOAuthService::build_http_client(config.custom_headers.as_ref())
            .map_err(|e| {
                AuthFlowError::CompletionFailed(format!("Failed to build HTTP client: {}", e))
            })?
            .get(token_refresh_url.as_str())
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                AuthFlowError::CompletionFailed(format!("API key exchange request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            if status.as_u16() == 403 {
                return Err(AuthFlowError::CompletionFailed(
                    "Access denied. Ensure you have an active subscription.".to_string(),
                ));
            }
            return Err(AuthFlowError::CompletionFailed(format!(
                "API key fetch failed ({}): {}",
                status,
                response.text().await.unwrap_or_default()
            )));
        }

        let OAuthTokenResponse { access_token, expires_at, .. } =
            response.json().await.map_err(|e| {
                AuthFlowError::CompletionFailed(format!("Failed to parse API key response: {}", e))
            })?;

        Ok((
            access_token.into(),
            chrono::DateTime::from_timestamp(expires_at.unwrap_or(0), 0)
                .unwrap_or_else(chrono::Utc::now),
        ))
    }

    /// Completes OAuth with API key flow by exchanging OAuth token for API key.
    ///
    /// This converts the OAuth access token into a time-limited API key using
    /// the token refresh URL. Both are stored in the credential for refresh.
    async fn handle_oauth_with_apikey_complete(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
        config: &crate::provider::OAuthConfig,
    ) -> Result<ProviderCredential, super::AuthFlowError> {
        use super::AuthFlowError;

        match result {
            AuthResult::OAuthTokens { access_token, refresh_token, expires_in: _ } => {
                // Exchange OAuth token for API key using config
                let (api_key, expires_at) = self
                    .exchange_oauth_for_api_key(&access_token, config)
                    .await?;

                // Create OAuth tokens structure
                let oauth_tokens = if let Some(refresh_tok) = refresh_token {
                    OAuthTokens::new(refresh_tok, access_token, expires_at)
                } else {
                    // Use access token as refresh token if none provided
                    OAuthTokens::new(access_token.as_str().to_string(), access_token, expires_at)
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
        let token_response =
            ForgeOAuthService::refresh_access_token(config, tokens.refresh_token.as_str())
                .await
                .map_err(|e| {
                    AuthFlowError::RefreshFailed(format!("Token refresh failed: {}", e))
                })?;

        // Use helpers for consistent handling
        let expires_at =
            Self::calculate_token_expiry(token_response.expires_in, chrono::Duration::days(30));

        let new_tokens = OAuthTokens::new(
            token_response
                .refresh_token
                .unwrap_or_else(|| tokens.refresh_token.as_str().to_string()),
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
        let token_response =
            ForgeOAuthService::refresh_access_token(config, oauth_tokens.refresh_token.as_str())
                .await
                .map_err(|e| {
                    AuthFlowError::RefreshFailed(format!("Failed to refresh access token: {}", e))
                })?;

        // Use helpers for consistent handling
        let expires_at =
            Self::calculate_token_expiry(token_response.expires_in, chrono::Duration::hours(1));

        // Create updated OAuth tokens
        let updated_tokens = OAuthTokens::new(
            oauth_tokens.refresh_token.clone(), // Keep original refresh token
            token_response.access_token,
            expires_at,
        );

        // Create new credential with refreshed tokens
        let mut refreshed = credential.clone();
        refreshed.update_oauth_tokens(updated_tokens);

        Ok(refreshed)
    }

    /// Handles OAuth with API key credential refresh (GitHub Copilot pattern)
    async fn handle_oauth_with_apikey_refresh(
        &self,
        credential: &ProviderCredential,
        config: &OAuthConfig,
    ) -> Result<ProviderCredential, AuthFlowError> {
        // Get stored OAuth tokens
        let oauth_tokens = credential.oauth_tokens.as_ref().ok_or_else(|| {
            AuthFlowError::RefreshFailed("Missing OAuth tokens in credential".to_string())
        })?;

        // Use the stored access token to fetch fresh API key using config
        let (new_api_key, expires_at) = self
            .exchange_oauth_for_api_key(&oauth_tokens.access_token, config)
            .await
            .map_err(|e| {
                AuthFlowError::RefreshFailed(format!("Failed to refresh API key: {}", e))
            })?;

        // Create updated OAuth tokens with new expiry
        let updated_tokens = OAuthTokens::new(
            oauth_tokens.refresh_token.as_str().to_string(),
            oauth_tokens.access_token.as_str().to_string(),
            expires_at,
        );

        // Create new credential with refreshed API key
        let mut refreshed = credential.clone();
        refreshed.api_key = Some(new_api_key);
        refreshed.oauth_tokens = Some(updated_tokens);

        Ok(refreshed)
    }
}

#[async_trait::async_trait]
impl<I> ProviderAuthService for ForgeProviderAuthService<I>
where
    I: ProviderCredentialRepository
        + EnvironmentInfra
        + AppConfigRepository
        + Send
        + Sync
        + 'static,
{
    async fn init_provider_auth(
        &self,
        provider_id: ProviderId,
        method: AuthMethod,
    ) -> anyhow::Result<forge_app::dto::AuthContext> {
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
            AuthMethod::OAuthDevice(config) => self
                .handle_oauth_device_init(config)
                .await
                .map_err(|e| anyhow::anyhow!(e)),
            AuthMethod::OAuthCode(config) => {
                // Handle OAuth code flow directly
                self.handle_oauth_code_init(&provider_id, config)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))
            }
        }
    }

    async fn refresh_provider_credential(
        &self,
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
                    self.handle_oauth_with_apikey_refresh(credential, config)
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

    async fn complete_provider_auth(
        &self,
        provider_id: ProviderId,
        context: AuthContext,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        let method = context.method();
        match context {
            AuthContext::ApiKey(ctx) => {
                let result = AuthResult::ApiKey {
                    api_key: ctx.response.api_key,
                    url_params: ctx.response.url_params,
                };
                self.complete_provider_auth_with_result(provider_id, result, method)
                    .await?;
                Ok(())
            }
            AuthContext::DeviceCode(_) => {
                let result = self
                    .poll_provider_auth(&context, timeout, method.clone())
                    .await?;
                self.complete_provider_auth_with_result(provider_id, result, method)
                    .await?;
                Ok(())
            }
            AuthContext::Code(_) => {
                let result = self
                    .poll_provider_auth(&context, timeout, method.clone())
                    .await?;
                self.complete_provider_auth_with_result(provider_id, result, method)
                    .await?;
                Ok(())
            }
        }
    }
}

impl<I> ForgeProviderAuthService<I>
where
    I: ProviderCredentialRepository
        + EnvironmentInfra
        + AppConfigRepository
        + Send
        + Sync
        + 'static,
{
    /// Polls until provider authentication completes (for OAuth flows)
    ///
    /// # Errors
    /// Returns error if polling fails, times out, or auth is denied
    async fn poll_provider_auth(
        &self,
        context: &AuthContext,
        timeout: Duration,
        method: AuthMethod,
    ) -> anyhow::Result<AuthResult> {
        // Dispatch based on auth method
        match &method {
            AuthMethod::ApiKey => {
                unimplemented!("API key method does not require polling")
            }
            AuthMethod::OAuthDevice(config) => {
                // Extract device code from context
                let (device_code, _interval) = context
                    .as_device_code()
                    .ok_or_else(|| anyhow::anyhow!("Invalid context type for device flow"))?;

                // Check if this needs OAuth with API key exchange (GitHub Copilot pattern)
                if config.token_refresh_url.is_some() {
                    // Handle OAuth with API key polling (GitHub-compatible)
                    self.handle_oauth_device_poll(device_code, config, timeout, true)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                } else {
                    // Handle OAuth device flow polling (standard OAuth)
                    self.handle_oauth_device_poll(device_code, config, timeout, false)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                }
            }
            AuthMethod::OAuthCode(_config) => {
                unimplemented!("OAuth code flow polling is not required")
            }
        }
    }

    /// Completes provider authentication and saves credential
    ///
    /// # Errors
    /// Returns error if credential creation or storage fails
    async fn complete_provider_auth_with_result(
        &self,
        provider_id: ProviderId,
        result: AuthResult,
        method: AuthMethod,
    ) -> anyhow::Result<ProviderCredential> {
        // Dispatch based on auth method and result type
        let credential = match (&method, &result) {
            (AuthMethod::ApiKey, AuthResult::ApiKey { api_key, url_params }) => {
                // Handle API key auth directly
                self.handle_api_key_complete(provider_id, api_key.clone(), url_params.clone())
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?
            }
            (AuthMethod::OAuthDevice(config), AuthResult::OAuthTokens { .. }) => {
                // Check if this needs OAuth with API key exchange (GitHub Copilot pattern)
                if config.token_refresh_url.is_some() {
                    // Handle OAuth with API key completion directly
                    self.handle_oauth_with_apikey_complete(provider_id, result, config)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?
                } else {
                    // Handle OAuth device flow completion directly
                    self.handle_oauth_device_complete(provider_id, result)
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
                    provider_id,
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
}
