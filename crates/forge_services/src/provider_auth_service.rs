use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use forge_app::ProviderAuthService;
use forge_domain::{
    ApiKey, ApiKeyRequest, AuthContext, AuthContextRequest, AuthContextResponse, AuthCredential,
    AuthorizationCode, CodeRequest, DeviceCodeRequest, DeviceCodeResponse, OAuthConfig,
    OAuthTokens, PkceVerifier, ProviderId, ProviderRepository, URLParam, URLParamValue,
};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode as OAuth2AuthCode, ClientId, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::Error;

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

fn build_auth_code_url(
    provider_id: &ProviderId,
    config: &OAuthConfig,
) -> anyhow::Result<AuthCodeParams> {
    // Check if this is Anthropic OAuth (non-standard: state = verifier)
    let is_anthropic = matches!(
        provider_id,
        ProviderId::Anthropic | ProviderId::AnthropicCompatible
    );

    if is_anthropic && config.use_pkce {
        // Anthropic requires state to be set to the PKCE verifier (non-standard)
        // Build URL manually instead of using oauth2 library
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

        let mut url = config.auth_url.clone();

        // Add required OAuth parameters
        url.query_pairs_mut()
            .append_pair("client_id", &config.client_id)
            .append_pair("response_type", "code")
            .append_pair("scope", &config.scopes.join(" "))
            .append_pair("code_challenge", challenge.as_str())
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", verifier.secret()); // ← Set state to verifier!

        // Add redirect_uri only if provided
        if let Some(redirect_uri) = &config.redirect_uri {
            url.query_pairs_mut()
                .append_pair("redirect_uri", redirect_uri);
        }

        // Add extra parameters (like code=true)
        if let Some(extra_params) = &config.extra_auth_params {
            for (key, value) in extra_params {
                url.query_pairs_mut().append_pair(key, value);
            }
        }

        return Ok(AuthCodeParams {
            auth_url: url.to_string(),
            state: verifier.secret().to_string(),
            code_verifier: Some(verifier.secret().to_string()),
        });
    }

    // Standard OAuth flow for other providers
    let mut client = BasicClient::new(ClientId::new(config.client_id.to_string()))
        .set_auth_uri(AuthUrl::new(config.auth_url.to_string())?)
        .set_token_uri(TokenUrl::new(config.token_url.to_string())?);

    // Add redirect_uri if provided
    if let Some(redirect_uri) = &config.redirect_uri {
        client = client.set_redirect_uri(RedirectUrl::new(redirect_uri.clone())?);
    }

    let mut request = client.authorize_url(CsrfToken::new_random);

    // Add scopes
    for scope in &config.scopes {
        request = request.add_scope(Scope::new(scope.clone()));
    }

    // Add extra authorization parameters (provider-specific)
    if let Some(extra_params) = &config.extra_auth_params {
        for (key, value) in extra_params {
            request = request.add_extra_param(key, value);
        }
    }

    // Add PKCE if configured
    let (auth_url, csrf_state, pkce_verifier) = if config.use_pkce {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state) = request.set_pkce_challenge(challenge).url();
        (url, state, Some(verifier))
    } else {
        let (url, state) = request.url();
        (url, state, None)
    };

    Ok(AuthCodeParams {
        auth_url: auth_url.to_string(),
        state: csrf_state.secret().to_string(),
        code_verifier: pkce_verifier.map(|v| v.secret().to_string()),
    })
}
fn build_http_client(
    custom_headers: Option<&HashMap<String, String>>,
) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        // Disable redirects to prevent SSRF vulnerabilities
        .redirect(reqwest::redirect::Policy::none());

    if let Some(headers) = custom_headers {
        let mut header_map = reqwest::header::HeaderMap::new();

        for (key, value) in headers {
            let header_name = reqwest::header::HeaderName::try_from(key.as_str())
                .map_err(|e| anyhow::anyhow!("Invalid header name '{}': {}", key, e))?;
            let header_value = value
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid header value for '{}': {}", key, e))?;
            header_map.insert(header_name, header_value);
        }

        builder = builder.default_headers(header_map);
    }

    Ok(builder.build()?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    /// Access token for API requests
    #[serde(alias = "token")]
    pub access_token: String,

    /// Refresh token for obtaining new access tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Seconds until access token expires
    #[serde(skip_serializing_if = "Option::is_none", alias = "refresh_in")]
    pub expires_in: Option<u64>,

    /// Unix timestamp when token expires (GitHub Copilot pattern)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,

    /// Token type (usually "Bearer")
    #[serde(default = "default_token_type")]
    pub token_type: String,

    /// OAuth scopes granted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

#[derive(Debug, Clone)]
pub struct AuthCodeParams {
    pub auth_url: String,
    pub state: String,
    pub code_verifier: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnthropicTokenRequest {
    /// Authorization code from callback
    code: String,
    /// State parameter (equals PKCE verifier)
    state: String,
    /// Must be "authorization_code"
    grant_type: String,
    /// OAuth client ID
    client_id: String,
    /// Redirect URI (must match authorization request)
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
    /// PKCE code verifier
    code_verifier: String,
}

impl<T: TokenResponse> From<T> for OAuthTokenResponse {
    fn from(token: T) -> Self {
        Self {
            access_token: token.access_token().secret().to_string(),
            refresh_token: token.refresh_token().map(|t| t.secret().to_string()),
            expires_in: token.expires_in().map(|d| d.as_secs()),
            expires_at: None,
            token_type: "Bearer".to_string(),
            scope: token.scopes().map(|scopes| {
                scopes
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        }
    }
}

#[derive(Clone)]
pub struct ForgeProviderAuthService<I> {
    infra: Arc<I>,
}

impl<I> ForgeProviderAuthService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn github_compliant_http_request(
        client: reqwest::Client,
        request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, reqwest::Error> {
        // Execute the request
        let mut req_builder = client
            .request(request.method().clone(), request.uri().to_string())
            .body(request.body().clone());

        for (name, value) in request.headers() {
            req_builder = req_builder.header(name.as_str(), value.as_bytes());
        }

        let response = req_builder.send().await?;

        // Get status and body
        let status_code = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await?;

        // GitHub-specific fix: If status is 200 but body contains "error" field,
        // change status to 400 so oauth2 crate recognizes it as an error response
        let fixed_status = if status_code.is_success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
                if json.get("error").is_some() {
                    // This is actually an error response masquerading as success
                    http::StatusCode::BAD_REQUEST
                } else {
                    status_code
                }
            } else {
                status_code
            }
        } else {
            status_code
        };

        // Build http::Response with corrected status
        let mut response_builder = http::Response::builder().status(fixed_status);

        // Add headers
        for (name, value) in headers.iter() {
            response_builder = response_builder.header(name, value);
        }

        Ok(response_builder
            .body(body.to_vec())
            .expect("Failed to build HTTP response"))
    }
    async fn handle_oauth_code_init(
        &self,
        provider_id: &ProviderId,
        config: &OAuthConfig,
    ) -> anyhow::Result<AuthContextRequest> {
        // Build authorization URL with PKCE
        let auth_params = build_auth_code_url(provider_id, config)
            .map_err(|e| Error::InitiationFailed(format!("Failed to build auth URL: {}", e)))?;

        // Build the type-safe context
        Ok(AuthContextRequest::Code(CodeRequest {
            authorization_url: Url::parse(&auth_params.auth_url)?,
            state: auth_params.state.into(),
            pkce_verifier: auth_params.code_verifier.map(Into::into),
            oauth_config: config.clone(),
        }))
    }
    async fn handle_oauth_device_init(
        &self,
        config: &OAuthConfig,
    ) -> anyhow::Result<AuthContextRequest> {
        // Validate configuration
        // Build oauth2 client
        use oauth2::basic::BasicClient;
        use oauth2::{ClientId, DeviceAuthorizationUrl, Scope, TokenUrl};
        let client = BasicClient::new(ClientId::new(config.client_id.to_string()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(config.auth_url.to_string())
                    .map_err(|e| Error::InitiationFailed(format!("Invalid auth_url: {}", e)))?,
            )
            .set_token_uri(
                TokenUrl::new(config.token_url.to_string())
                    .map_err(|e| Error::InitiationFailed(format!("Invalid token_url: {}", e)))?,
            );

        // Request device authorization
        let mut request = client.exchange_device_code();
        for scope in &config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }

        // Build HTTP client with custom headers
        let http_client = build_http_client(config.custom_headers.as_ref())
            .map_err(|e| Error::InitiationFailed(format!("Failed to build HTTP client: {}", e)))?;

        let http_fn = |req| Self::github_compliant_http_request(http_client.clone(), req);

        let device_auth_response: oauth2::StandardDeviceAuthorizationResponse =
            request.request_async(&http_fn).await.map_err(|e| {
                Error::InitiationFailed(format!("Device authorization request failed: {}", e))
            })?;

        // Build the type-safe context
        Ok(AuthContextRequest::DeviceCode(DeviceCodeRequest {
            user_code: device_auth_response.user_code().secret().to_string().into(),
            device_code: device_auth_response
                .device_code()
                .secret()
                .to_string()
                .into(),
            verification_uri: Url::parse(&device_auth_response.verification_uri().to_string())?,
            verification_uri_complete: device_auth_response
                .verification_uri_complete()
                .map(|u| Url::parse(&u.secret().to_string()).unwrap()),
            expires_in: device_auth_response.expires_in().as_secs(),
            interval: device_auth_response.interval().as_secs(),
            oauth_config: config.clone(),
        }))
    }
    async fn handle_api_key_init(
        &self,
        required_params: Vec<URLParam>,
    ) -> anyhow::Result<AuthContextRequest> {
        Ok(AuthContextRequest::ApiKey(ApiKeyRequest {
            required_params,
        }))
    }
    async fn handle_api_key_complete(
        &self,
        provider_id: ProviderId,
        api_key: ApiKey,
        url_params: HashMap<URLParam, URLParamValue>,
    ) -> anyhow::Result<AuthCredential> {
        Ok(AuthCredential::new_api_key(provider_id, api_key).url_params(url_params))
    }

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
    fn handle_oauth_error(error_code: &str) -> Result<(), Error> {
        match error_code {
            "authorization_pending" | "slow_down" => Ok(()),
            "expired_token" => Err(Error::Expired),
            "access_denied" => Err(Error::Denied),
            _ => Err(Error::PollFailed(format!("OAuth error: {}", error_code))),
        }
    }

    fn parse_token_response(body: &str) -> Result<(String, Option<String>, Option<u64>), Error> {
        let token_response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| Error::PollFailed(format!("Failed to parse token response: {}", e)))?;

        let access_token = token_response["access_token"]
            .as_str()
            .ok_or_else(|| Error::PollFailed("Missing access_token in response".to_string()))?
            .to_string();

        let refresh_token = token_response["refresh_token"]
            .as_str()
            .map(|s| s.to_string());

        let expires_in = token_response["expires_in"].as_u64();

        Ok((access_token, refresh_token, expires_in))
    }
    async fn exchange_oauth_for_api_key(
        &self,
        oauth_token: &str,
        config: &OAuthConfig,
    ) -> anyhow::Result<(ApiKey, chrono::DateTime<chrono::Utc>)> {
        let token_refresh_url = config.token_refresh_url.as_ref().ok_or_else(|| {
            Error::CompletionFailed("Missing token_refresh_url in config".to_string())
        })?;

        // Build request headers
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", oauth_token)).map_err(
                |e| Error::CompletionFailed(format!("Invalid authorization header: {}", e)),
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

        let response = build_http_client(config.custom_headers.as_ref())
            .map_err(|e| Error::CompletionFailed(format!("Failed to build HTTP client: {}", e)))?
            .get(token_refresh_url.as_str())
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                Error::CompletionFailed(format!("API key exchange request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            if status.as_u16() == 403 {
                return Err(Error::CompletionFailed(
                    "Access denied. Ensure you have an active subscription.".to_string(),
                )
                .into());
            }
            return Err(Error::CompletionFailed(format!(
                "API key fetch failed ({}): {}",
                status,
                response.text().await.unwrap_or_default()
            ))
            .into());
        }

        let OAuthTokenResponse { access_token, expires_at, .. } =
            response.json().await.map_err(|e| {
                Error::CompletionFailed(format!("Failed to parse API key response: {}", e))
            })?;

        Ok((
            access_token.into(),
            chrono::DateTime::from_timestamp(expires_at.unwrap_or(0), 0)
                .unwrap_or_else(chrono::Utc::now),
        ))
    }
    async fn handle_oauth_with_apikey_complete(
        &self,
        provider_id: ProviderId,
        token_response: OAuthTokenResponse,
        config: &OAuthConfig,
    ) -> anyhow::Result<AuthCredential> {
        // Exchange OAuth token for API key using config
        let (api_key, expires_at) = self
            .exchange_oauth_for_api_key(&token_response.access_token, config)
            .await?;

        // Create OAuth tokens structure
        let oauth_tokens = if let Some(_refresh_tok) = &token_response.refresh_token {
            OAuthTokens::new(
                token_response.access_token,
                token_response.refresh_token,
                expires_at,
            )
        } else {
            // Use access token as refresh token if none provided
            OAuthTokens::new(
                token_response.access_token,
                token_response.refresh_token,
                expires_at,
            )
        };

        // Create credential with both OAuth token and API key
        let credential = AuthCredential::new_oauth_with_api_key(
            provider_id,
            oauth_tokens,
            api_key,
            config.clone(),
        );

        Ok(credential)
    }
    async fn exchange_anthropic_token(
        token_url: &str,
        auth_code: &str,
        code_verifier: Option<&str>,
        config: &OAuthConfig,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Parse code#state format
        let (code, state) = if auth_code.contains('#') {
            let parts: Vec<&str> = auth_code.split('#').collect();
            (parts[0].to_string(), parts.get(1).map(|s| s.to_string()))
        } else {
            (auth_code.to_string(), None)
        };

        let verifier = code_verifier
            .ok_or_else(|| anyhow::anyhow!("PKCE verifier required for Anthropic OAuth"))?;

        // Build request body using concrete type
        let request_body = AnthropicTokenRequest {
            code,
            state: state.unwrap_or_else(|| verifier.to_string()),
            grant_type: "authorization_code".to_string(),
            client_id: config.client_id.to_string(),
            redirect_uri: config.redirect_uri.clone(),
            code_verifier: verifier.to_string(),
        };

        // Build HTTP client
        let client = build_http_client(config.custom_headers.as_ref())?;

        let response = client
            .post(token_url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Token exchange failed with status {}: {}",
                status,
                error_text
            );
        }

        Ok(response.json().await?)
    }
    pub async fn exchange_auth_code(
        provider_id: &ProviderId,
        config: &OAuthConfig,
        auth_code: &str,
        code_verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let is_anthropic = matches!(provider_id, ProviderId::Anthropic);

        if is_anthropic {
            return Self::exchange_anthropic_token(
                config.token_url.as_str(),
                auth_code,
                code_verifier,
                config,
            )
            .await;
        }

        let mut client = BasicClient::new(ClientId::new(config.client_id.to_string()))
            .set_auth_uri(AuthUrl::new(config.auth_url.to_string())?)
            .set_token_uri(TokenUrl::new(config.token_url.to_string())?);

        if let Some(redirect_uri) = &config.redirect_uri {
            client = client.set_redirect_uri(RedirectUrl::new(redirect_uri.clone())?);
        }

        let http_client = build_http_client(config.custom_headers.as_ref())?;

        let code = OAuth2AuthCode::new(auth_code.to_string());

        let mut request = client.exchange_code(code);

        if let Some(verifier) = code_verifier {
            request = request.set_pkce_verifier(PkceCodeVerifier::new(verifier.to_string()));
        }

        let token_result = request.request_async(&http_client).await?;

        Ok(token_result.into())
    }
    async fn handle_oauth_code_complete(
        &self,
        provider_id: ProviderId,
        code: AuthorizationCode,
        code_verifier: Option<PkceVerifier>,
        config: &OAuthConfig,
    ) -> anyhow::Result<AuthCredential> {
        // Exchange code for tokens with PKCE verifier (if provided)
        let token_response = Self::exchange_auth_code(
            &provider_id,
            config,
            code.as_str(),
            code_verifier.as_ref().map(|v| v.as_str()),
        )
        .await
        .map_err(|e| {
            Error::CompletionFailed(format!("Failed to exchange authorization code: {}", e))
        })?;

        // Use helpers for consistent credential building
        let expires_at =
            calculate_token_expiry(token_response.expires_in, chrono::Duration::hours(1));
        let oauth_tokens = OAuthTokens::new(
            token_response.access_token,
            token_response.refresh_token,
            expires_at,
        );
        Ok(AuthCredential::new_oauth(
            provider_id,
            oauth_tokens,
            config.clone(),
        ))
    }
    async fn handle_oauth_device_complete(
        &self,
        provider_id: ProviderId,
        token_response: OAuthTokenResponse,
        config: &OAuthConfig,
    ) -> anyhow::Result<AuthCredential> {
        // Use helpers for consistent credential building
        let expires_at =
            calculate_token_expiry(token_response.expires_in, chrono::Duration::days(365));
        let oauth_tokens = OAuthTokens::new(
            token_response.access_token,
            token_response.refresh_token,
            expires_at,
        );
        Ok(AuthCredential::new_oauth(
            provider_id,
            oauth_tokens,
            config.clone(),
        ))
    }

    async fn handle_oauth_device_poll(
        &self,
        device_code: &str,
        config: &OAuthConfig,
        timeout: Duration,
        github_compatible: bool,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let http_client = build_http_client(config.custom_headers.as_ref())
            .map_err(|e| Error::PollFailed(format!("Failed to build HTTP client: {}", e)))?;

        use reqwest::header::{HeaderMap, HeaderValue};

        let start_time = tokio::time::Instant::now();
        let interval = Duration::from_secs(5);

        loop {
            // Check timeout
            if start_time.elapsed() >= timeout {
                return Err(Error::Timeout(timeout).into());
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

            let body = serde_urlencoded::to_string(&params)
                .map_err(|e| Error::PollFailed(format!("Failed to encode request: {}", e)))?;

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
                .map_err(|e| Error::PollFailed(format!("HTTP request failed: {}", e)))?;

            let status = response.status();
            let body_text = response
                .text()
                .await
                .map_err(|e| Error::PollFailed(format!("Failed to read response: {}", e)))?;

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
                    return Err(Self::handle_oauth_error(error).unwrap_err().into());
                }

                // No error field - parse as success
                let (access_token, refresh_token, expires_in) =
                    Self::parse_token_response(&body_text)?;

                return Ok(OAuthTokenResponse {
                    access_token,
                    refresh_token,
                    expires_in,
                    expires_at: None,
                    token_type: "Bearer".to_string(),
                    scope: None,
                });
            }

            // Standard OAuth: HTTP success means tokens
            if !github_compatible && status.is_success() {
                let (access_token, refresh_token, expires_in) =
                    Self::parse_token_response(&body_text)?;

                return Ok(OAuthTokenResponse {
                    access_token,
                    refresh_token,
                    expires_in,
                    expires_at: None,
                    token_type: "Bearer".to_string(),
                    scope: None,
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
                return Err(Self::handle_oauth_error(error).unwrap_err().into());
            }

            // Unknown error
            return Err(Error::PollFailed(format!("HTTP {}: {}", status, body_text)).into());
        }
    }
    async fn poll_provider_auth(
        &self,
        context: &AuthContext<DeviceCodeRequest, DeviceCodeResponse>,
        timeout: Duration,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let config = &context.request.oauth_config;
        let device_code = &context.request.device_code;

        // Check if this needs OAuth with API key exchange (GitHub Copilot pattern)
        if config.token_refresh_url.is_some() {
            // Handle OAuth with API key polling (GitHub-compatible)
            Ok(self
                .handle_oauth_device_poll(device_code, config, timeout, true)
                .await?)
        } else {
            // Handle OAuth device flow polling (standard OAuth)
            Ok(self
                .handle_oauth_device_poll(device_code, config, timeout, false)
                .await?)
        }
    }
    pub async fn refresh_access_token(
        config: &OAuthConfig,
        refresh_token: &str,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Get token URL from config
        // Build minimal oauth2 client (just need token endpoint)
        let client = BasicClient::new(ClientId::new(config.client_id.to_string()))
            .set_token_uri(TokenUrl::new(config.token_url.to_string())?);

        // Build HTTP client with custom headers
        let http_client = build_http_client(config.custom_headers.as_ref())?;

        let refresh_token = RefreshToken::new(refresh_token.to_string());

        // Use GitHub-compliant HTTP function to handle non-RFC responses
        let http_fn = |req| Self::github_compliant_http_request(http_client.clone(), req);

        let token_result = client
            .exchange_refresh_token(&refresh_token)
            .request_async(&http_fn)
            .await?;

        Ok(token_result.into())
    }
    async fn handle_oauth_device_refresh(
        &self,
        credential: &AuthCredential,
        config: &OAuthConfig,
    ) -> anyhow::Result<AuthCredential> {
        let id = credential.id;
        let oauth_tokens = match &credential.auth_details {
            forge_domain::AuthDetails::OAuth { tokens, .. } => Some(tokens),
            _ => None,
        };
        if let Some(refresh_token) = oauth_tokens.and_then(|x| x.refresh_token.clone()) {
            let token_response = Self::refresh_access_token(config, refresh_token.as_str())
                .await
                .map_err(|e| Error::RefreshFailed(format!("Token refresh failed: {}", e)))?;

            // Use helpers for consistent handling
            let expires_at =
                calculate_token_expiry(token_response.expires_in, chrono::Duration::days(30));

            let new_tokens = OAuthTokens::new(
                token_response.access_token,
                token_response.refresh_token,
                expires_at,
            );
            Ok(AuthCredential::new_oauth(id, new_tokens, config.clone()))
        } else {
            Err(Error::RefreshFailed(
                "No refresh token available for OAuth token refresh".to_string(),
            )
            .into())
        }
        // Use OAuth service to refresh token
    }
    async fn handle_oauth_with_apikey_refresh(
        &self,
        credential: &AuthCredential,
        config: &OAuthConfig,
    ) -> anyhow::Result<AuthCredential> {
        let oauth_tokens = match &credential.auth_details {
            forge_domain::AuthDetails::OAuthWithApiKey { tokens, .. } => Some(tokens),
            _ => None,
        };

        // Get stored OAuth tokens
        let oauth_tokens = oauth_tokens.as_ref().ok_or_else(|| {
            Error::RefreshFailed("Missing OAuth tokens in credential".to_string())
        })?;

        if let Some(refresh_token) = &oauth_tokens.refresh_token {
            let token_response = Self::refresh_access_token(config, refresh_token.as_str()).await?;

            // Use the refreshed access token to fetch fresh API key
            let (new_api_key, expires_at) = self
                .exchange_oauth_for_api_key(&token_response.access_token, config)
                .await?;

            // Create updated OAuth tokens with refreshed access token and new expiry
            let updated_tokens = OAuthTokens::new(
                token_response.access_token,
                token_response.refresh_token,
                expires_at,
            );

            Ok(AuthCredential::new_oauth_with_api_key(
                credential.id,
                updated_tokens,
                new_api_key,
                config.clone(),
            ))
        } else {
            Err(Error::RefreshFailed(
                "No refresh token available for OAuth token refresh".to_string(),
            )
            .into())
        }
    }
}
#[async_trait::async_trait]
impl<I> ProviderAuthService for ForgeProviderAuthService<I>
where
    I: ProviderRepository + Send + Sync + 'static,
{
    async fn init_provider_auth(
        &self,
        provider_id: forge_domain::ProviderId,
        auth_method: forge_domain::AuthMethod,
    ) -> anyhow::Result<forge_domain::AuthContextRequest> {
        match auth_method {
            forge_domain::AuthMethod::ApiKey => {
                let required_params = self
                    .infra
                    .get_provider(provider_id)
                    .await?
                    .url_params
                    .clone();
                self.handle_api_key_init(required_params).await
            }
            forge_domain::AuthMethod::OAuthDevice(config) => {
                self.handle_oauth_device_init(&config).await
            }
            forge_domain::AuthMethod::OAuthCode(config) => {
                self.handle_oauth_code_init(&provider_id, &config).await
            }
        }
    }

    async fn complete_provider_auth(
        &self,
        provider_id: forge_domain::ProviderId,
        auth_context_response: forge_domain::AuthContextResponse,
        timeout: Duration,
    ) -> anyhow::Result<()> {
        match auth_context_response {
            AuthContextResponse::ApiKey(ctx) => {
                let credential = self
                    .handle_api_key_complete(
                        provider_id,
                        ctx.response.api_key,
                        ctx.response.url_params,
                    )
                    .await?;
                self.infra.upsert_credential(credential).await
            }
            AuthContextResponse::DeviceCode(ctx) => {
                let token_response = self.poll_provider_auth(&ctx, timeout).await?;

                let config = &ctx.request.oauth_config;

                // Dispatch based on auth method
                if config.token_refresh_url.is_some() {
                    // Handle OAuth with API key completion
                    self.handle_oauth_with_apikey_complete(provider_id, token_response, config)
                        .await?;
                } else {
                    self.handle_oauth_device_complete(provider_id, token_response, config)
                        .await?;
                }
                Ok(())
            }
            AuthContextResponse::Code(ctx) => {
                let code = ctx.response.code.clone();
                let pkce_verifier = ctx.request.pkce_verifier.clone();

                // Handle OAuth code flow completion
                self.handle_oauth_code_complete(
                    provider_id,
                    code,
                    pkce_verifier,
                    &ctx.request.oauth_config,
                )
                .await?;

                Ok(())
            }
        }
    }

    async fn refresh_provider_credential(
        &self,
        provider: &forge_domain::Provider<url::Url>,
        auth_method: forge_domain::AuthMethod,
    ) -> anyhow::Result<forge_domain::AuthCredential> {
        match auth_method {
            forge_domain::AuthMethod::ApiKey => self
                .infra
                .get_credential(&provider.id)
                .await?
                .ok_or_else(|| {
                    forge_domain::Error::ProviderNotAvailable { provider: provider.id }.into()
                }),
            forge_domain::AuthMethod::OAuthDevice(config) => {
                let credential =
                    self.infra
                        .get_credential(&provider.id)
                        .await?
                        .ok_or_else(|| forge_domain::Error::ProviderNotAvailable {
                            provider: provider.id,
                        })?;
                if config.token_refresh_url.is_some() {
                    // OAuth with API key refresh
                    self.handle_oauth_with_apikey_refresh(&credential, &config)
                        .await
                } else {
                    // Standard OAuth device flow refresh
                    self.handle_oauth_device_refresh(&credential, &config).await
                }
            }
            forge_domain::AuthMethod::OAuthCode(_) => {
                // OAuth refresh logic would go here
                todo!("OAuth refresh not implemented yet")
            }
        }
    }
}
