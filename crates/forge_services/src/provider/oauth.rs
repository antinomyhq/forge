/// OAuth flow implementation for provider authentication
///
/// Uses the oauth2 crate for RFC-compliant OAuth flows.
/// Supports both device authorization flow and authorization code flow.
use std::collections::HashMap;

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};

use crate::provider::OAuthConfig;

/// OAuth token response (both device and code flows)
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

/// Anthropic-specific token exchange request body
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

/// Parameters for building authorization code URL
#[derive(Debug, Clone)]
pub struct AuthCodeParams {
    /// Full authorization URL with query parameters
    pub auth_url: String,

    /// Random state for CSRF protection
    pub state: String,

    /// PKCE code verifier (store securely, needed for token exchange)
    pub code_verifier: Option<String>,
}

/// OAuth service for handling device and code flows using oauth2 crate
#[derive(Clone, Default)]
pub struct ForgeOAuthService;

impl ForgeOAuthService {
    /// Builds a reqwest HTTP client with custom headers from config
    pub fn build_http_client(
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

    /// Custom async HTTP function that fixes GitHub's non-compliant OAuth
    /// responses
    pub async fn github_compliant_http_request(
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

    /// Builds authorization URL for code flow
    pub fn build_auth_code_url(config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        // Check if this is Anthropic OAuth (non-standard: state = verifier)
        let is_anthropic = config.auth_url.contains("claude.ai/oauth");

        if is_anthropic && config.use_pkce {
            // Anthropic requires state to be set to the PKCE verifier (non-standard)
            // Build URL manually instead of using oauth2 library
            let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

            let mut url = url::Url::parse(&config.auth_url)?;

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
        let mut client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_auth_uri(AuthUrl::new(config.auth_url.clone())?)
            .set_token_uri(TokenUrl::new(config.token_url.clone())?);

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

    /// Exchanges authorization code for tokens
    pub async fn exchange_auth_code(
        config: &OAuthConfig,
        auth_code: &str,
        code_verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Check if this is Anthropic OAuth (requires special handling)
        let is_anthropic = config.auth_url.contains("claude.ai/oauth");

        if is_anthropic {
            // Anthropic requires JSON body with code, state, and code_verifier
            return Self::exchange_anthropic_token(
                &config.token_url,
                auth_code,
                code_verifier,
                config,
            )
            .await;
        }

        // Standard OAuth flow for other providers
        let mut client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_auth_uri(AuthUrl::new(config.auth_url.clone())?)
            .set_token_uri(TokenUrl::new(config.token_url.clone())?);

        // Add redirect_uri if provided
        if let Some(redirect_uri) = &config.redirect_uri {
            client = client.set_redirect_uri(RedirectUrl::new(redirect_uri.clone())?);
        }

        // Build HTTP client with custom headers
        let http_client = Self::build_http_client(config.custom_headers.as_ref())?;

        let code = AuthorizationCode::new(auth_code.to_string());

        // Build token exchange request
        let mut request = client.exchange_code(code);

        // Add PKCE verifier if provided
        if let Some(verifier) = code_verifier {
            request = request.set_pkce_verifier(PkceCodeVerifier::new(verifier.to_string()));
        }

        // Execute token exchange
        let token_result = request.request_async(&http_client).await?;

        Ok(token_result.into())
    }

    /// Exchanges authorization code for tokens using Anthropic's custom format
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
            client_id: config.client_id.clone(),
            redirect_uri: config.redirect_uri.clone(),
            code_verifier: verifier.to_string(),
        };

        // Build HTTP client
        let client = Self::build_http_client(config.custom_headers.as_ref())?;

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

    /// Refreshes access token using refresh token
    pub async fn refresh_access_token(
        config: &OAuthConfig,
        refresh_token: &str,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Get token URL from config
        // Build minimal oauth2 client (just need token endpoint)
        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_token_uri(TokenUrl::new(config.token_url.clone())?);

        // Build HTTP client with custom headers
        let http_client = Self::build_http_client(config.custom_headers.as_ref())?;

        let refresh_token = RefreshToken::new(refresh_token.to_string());

        // Use GitHub-compliant HTTP function to handle non-RFC responses
        let http_fn = |req| Self::github_compliant_http_request(http_client.clone(), req);

        let token_result = client
            .exchange_refresh_token(&refresh_token)
            .request_async(&http_fn)
            .await?;

        Ok(token_result.into())
    }
}

#[cfg(test)]
mod tests {
    use mockito::Server;
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_build_auth_code_url_with_pkce() {
        let config = OAuthConfig {
            auth_url: "https://provider.com/authorize".to_string(),
            token_url: "https://provider.com/token".to_string(),
            client_id: "test-client".to_string(),
            scopes: vec!["user:profile".to_string(), "user:email".to_string()],
            redirect_uri: Some("https://provider.com/callback".to_string()),
            use_pkce: true,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        };

        let params = ForgeOAuthService::build_auth_code_url(&config).unwrap();

        assert!(params.auth_url.contains("client_id=test-client"));
        assert!(params.auth_url.contains("response_type=code"));
        assert!(
            params
                .auth_url
                .contains("scope=user%3Aprofile+user%3Aemail")
        );
        assert!(params.auth_url.contains("code_challenge="));
        assert!(params.auth_url.contains("code_challenge_method=S256"));
        assert!(params.code_verifier.is_some());
        assert!(!params.state.is_empty());
    }

    #[tokio::test]
    async fn test_build_auth_code_url_without_pkce() {
        let config = OAuthConfig {
            auth_url: "https://provider.com/authorize".to_string(),
            token_url: "https://provider.com/token".to_string(),
            client_id: "test-client".to_string(),
            scopes: vec!["read".to_string()],
            redirect_uri: Some("https://provider.com/callback".to_string()),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        };

        let params = ForgeOAuthService::build_auth_code_url(&config).unwrap();

        assert!(!params.auth_url.contains("code_challenge"));
        assert!(params.code_verifier.is_none());
    }

    #[tokio::test]
    async fn test_exchange_auth_code_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_body(
                r#"{
                    "access_token": "test_access_token",
                    "refresh_token": "test_refresh_token",
                    "expires_in": 3600,
                    "token_type": "Bearer"
                }"#,
            )
            .create_async()
            .await;

        let config = OAuthConfig {
            auth_url: "https://provider.com/auth".to_string(),
            token_url: format!("{}/token", server.url()),
            client_id: "test-client".to_string(),
            scopes: vec![],
            redirect_uri: Some("https://provider.com/callback".to_string()),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
            extra_auth_params: None,
        };

        let response = ForgeOAuthService::exchange_auth_code(&config, "test_auth_code", None)
            .await
            .unwrap();

        assert_eq!(response.access_token, "test_access_token");
        assert_eq!(
            response.refresh_token,
            Some("test_refresh_token".to_string())
        );
        assert_eq!(response.expires_in, Some(3600));
        assert_eq!(response.token_type, "Bearer");

        mock.assert_async().await;
    }
}
