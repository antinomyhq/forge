/// OAuth flow implementation for provider authentication
///
/// Uses the oauth2 crate for RFC-compliant OAuth flows.
/// Supports both device authorization flow and authorization code flow.
use std::collections::HashMap;

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, DeviceAuthorizationUrl, EmptyExtraDeviceAuthorizationFields, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, StandardDeviceAuthorizationResponse,
    TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};

use crate::provider::OAuthConfig;

/// Response from device authorization initiation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthorizationResponse {
    /// Device verification code for polling
    pub device_code: String,

    /// User code to display (8-character format like "ABCD-1234")
    pub user_code: String,

    /// URL where user should visit to authorize
    pub verification_uri: String,

    /// Alternative URI with user_code embedded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri_complete: Option<String>,

    /// Seconds until device_code expires
    pub expires_in: u64,

    /// Minimum seconds to wait between polling attempts
    pub interval: u64,
}

/// OAuth token response (both device and code flows)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    /// Access token for API requests
    pub access_token: String,

    /// Refresh token for obtaining new access tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Seconds until access token expires
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,

    /// Token type (usually "Bearer")
    pub token_type: String,

    /// OAuth scopes granted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
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
#[derive(Clone)]
pub struct ForgeOAuthService;

impl ForgeOAuthService {
    /// Creates a new OAuth service
    pub fn new() -> Self {
        Self
    }

    /// Builds a reqwest HTTP client with custom headers from config
    ///
    /// # Arguments
    /// * `custom_headers` - Optional map of custom headers to include in all
    ///   requests
    ///
    /// # Returns
    /// Configured reqwest client with custom headers set as defaults
    fn build_http_client(
        &self,
        custom_headers: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<reqwest::Client> {
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

            Ok(reqwest::Client::builder()
                .default_headers(header_map)
                // Disable redirects to prevent SSRF vulnerabilities
                .redirect(reqwest::redirect::Policy::none())
                .build()?)
        } else {
            Ok(reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()?)
        }
    }

    /// Initiates device authorization flow
    ///
    /// # Arguments
    /// * `config` - OAuth configuration with device_code_url and client_id
    ///
    /// # Returns
    /// Device authorization response with user_code and verification_uri
    ///
    /// # Errors
    /// Returns error if HTTP request fails or response is invalid
    pub async fn initiate_device_auth(
        &self,
        config: &OAuthConfig,
    ) -> anyhow::Result<DeviceAuthorizationResponse> {
        let device_code_url = config
            .device_code_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("device_code_url not configured"))?;
        let device_token_url = config
            .device_token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("device_token_url not configured"))?;

        // Build oauth2 client for device flow
        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_device_authorization_url(DeviceAuthorizationUrl::new(device_code_url.clone())?)
            .set_token_uri(TokenUrl::new(device_token_url.clone())?);

        // Build HTTP client with custom headers
        let http_client = self.build_http_client(config.custom_headers.as_ref())?;

        // Build device authorization request with scopes
        let mut request = client.exchange_device_code();
        for scope in &config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }

        // Execute the request
        let details: StandardDeviceAuthorizationResponse =
            request.request_async(&http_client).await?;

        Ok(DeviceAuthorizationResponse {
            device_code: details.device_code().secret().to_string(),
            user_code: details.user_code().secret().to_string(),
            verification_uri: details.verification_uri().to_string(),
            verification_uri_complete: details
                .verification_uri_complete()
                .map(|uri| uri.secret().to_string()),
            expires_in: details.expires_in().as_secs(),
            interval: details.interval().as_secs(),
        })
    }

    /// Polls for device authorization completion
    ///
    /// Uses oauth2 crate's automatic polling with exponential backoff.
    /// The crate handles authorization_pending and slow_down errors automatically.
    ///
    /// # Arguments
    /// * `config` - OAuth configuration with device_token_url
    /// * `device_auth_response` - Full device authorization response from
    ///   `initiate_device_auth()`
    ///
    /// # Returns
    /// OAuth tokens once user authorizes
    ///
    /// # Errors
    /// Returns error if authorization fails, expires, or times out
    pub async fn poll_device_auth(
        &self,
        config: &OAuthConfig,
        device_auth_response: &DeviceAuthorizationResponse,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let device_code_url = config
            .device_code_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("device_code_url not configured"))?;
        let device_token_url = config
            .device_token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("device_token_url not configured"))?;

        // Build oauth2 client for device flow
        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_device_authorization_url(DeviceAuthorizationUrl::new(device_code_url.clone())?)
            .set_token_uri(TokenUrl::new(device_token_url.clone())?);

        // Build HTTP client with custom headers
        let http_client = self.build_http_client(config.custom_headers.as_ref())?;

        // Reconstruct the StandardDeviceAuthorizationResponse from our response
        // We need this because oauth2 crate's exchange_device_access_token expects it
        let details = StandardDeviceAuthorizationResponse {
            device_code: oauth2::DeviceCode::new(device_auth_response.device_code.clone()),
            user_code: oauth2::UserCode::new(device_auth_response.user_code.clone()),
            verification_uri: oauth2::EndUserVerificationUrl::new(
                device_auth_response.verification_uri.clone(),
            )?,
            verification_uri_complete: device_auth_response
                .verification_uri_complete
                .as_ref()
                .map(|uri| oauth2::VerificationUriComplete::new(uri.clone())),
            expires_in: device_auth_response.expires_in,
            interval: device_auth_response.interval,
            extra_fields: EmptyExtraDeviceAuthorizationFields {},
        };

        // Poll for token with automatic backoff - oauth2 crate handles the polling loop
        // Pass None for unlimited polling (will respect expires_in from details)
        let token_result = client
            .exchange_device_access_token(&details)
            .request_async(&http_client, tokio::time::sleep, None)
            .await?;

        Ok(OAuthTokenResponse {
            access_token: token_result.access_token().secret().to_string(),
            refresh_token: token_result.refresh_token().map(|t| t.secret().to_string()),
            expires_in: token_result.expires_in().map(|d| d.as_secs()),
            token_type: "Bearer".to_string(),
            scope: token_result.scopes().map(|scopes| {
                scopes
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        })
    }

    /// Builds authorization URL for code flow
    ///
    /// Generates URL with state and optionally PKCE parameters.
    ///
    /// # Arguments
    /// * `config` - OAuth configuration with auth_url, client_id, etc.
    ///
    /// # Returns
    /// Authorization parameters including URL, state, and optional code
    /// verifier
    ///
    /// # Errors
    /// Returns error if URL building fails
    pub fn build_auth_code_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        let auth_url = config
            .auth_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("auth_url not configured"))?;
        let token_url = config
            .token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("token_url not configured"))?;

        // Build oauth2 client for authorization code flow
        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_auth_uri(AuthUrl::new(auth_url.clone())?)
            .set_token_uri(TokenUrl::new(token_url.clone())?)
            .set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone())?);

        let mut request = client.authorize_url(CsrfToken::new_random);

        // Add scopes
        for scope in &config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
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
    ///
    /// # Arguments
    /// * `config` - OAuth configuration with token_url
    /// * `auth_code` - Authorization code from user
    /// * `code_verifier` - PKCE verifier (if PKCE was used)
    ///
    /// # Returns
    /// OAuth tokens (access_token, refresh_token, etc.)
    ///
    /// # Errors
    /// Returns error if HTTP request fails or code is invalid
    pub async fn exchange_auth_code(
        &self,
        config: &OAuthConfig,
        auth_code: &str,
        code_verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let auth_url = config
            .auth_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("auth_url not configured"))?;
        let token_url = config
            .token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("token_url not configured"))?;

        // Build oauth2 client for authorization code flow
        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_auth_uri(AuthUrl::new(auth_url.clone())?)
            .set_token_uri(TokenUrl::new(token_url.clone())?)
            .set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone())?);

        // Build HTTP client with custom headers
        let http_client = self.build_http_client(config.custom_headers.as_ref())?;

        let code = AuthorizationCode::new(auth_code.to_string());

        // Build token exchange request
        let mut request = client.exchange_code(code);

        // Add PKCE verifier if provided
        if let Some(verifier) = code_verifier {
            request = request.set_pkce_verifier(PkceCodeVerifier::new(verifier.to_string()));
        }

        // Execute token exchange
        let token_result = request.request_async(&http_client).await?;

        Ok(OAuthTokenResponse {
            access_token: token_result.access_token().secret().to_string(),
            refresh_token: token_result.refresh_token().map(|t| t.secret().to_string()),
            expires_in: token_result.expires_in().map(|d| d.as_secs()),
            token_type: "Bearer".to_string(),
            scope: token_result.scopes().map(|scopes| {
                scopes
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        })
    }

    /// Refreshes access token using refresh token
    ///
    /// # Arguments
    /// * `config` - OAuth configuration with token_url
    /// * `refresh_token` - Refresh token from previous authorization
    ///
    /// # Returns
    /// New OAuth tokens
    ///
    /// # Errors
    /// Returns error if refresh token is invalid or expired
    pub async fn refresh_access_token(
        &self,
        config: &OAuthConfig,
        refresh_token: &str,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Try code flow token URL first, fallback to device flow token URL
        let token_url = config
            .token_url
            .as_ref()
            .or(config.device_token_url.as_ref())
            .ok_or_else(|| anyhow::anyhow!("token_url not configured for refresh"))?;

        // Build minimal oauth2 client (just need token endpoint)
        let client = BasicClient::new(ClientId::new(config.client_id.clone()))
            .set_token_uri(TokenUrl::new(token_url.clone())?);

        // Build HTTP client with custom headers
        let http_client = self.build_http_client(config.custom_headers.as_ref())?;

        let refresh_token = RefreshToken::new(refresh_token.to_string());

        let token_result = client
            .exchange_refresh_token(&refresh_token)
            .request_async(&http_client)
            .await?;

        Ok(OAuthTokenResponse {
            access_token: token_result.access_token().secret().to_string(),
            refresh_token: token_result.refresh_token().map(|t| t.secret().to_string()),
            expires_in: token_result.expires_in().map(|d| d.as_secs()),
            token_type: "Bearer".to_string(),
            scope: token_result.scopes().map(|scopes| {
                scopes
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        })
    }
}

impl Default for ForgeOAuthService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use mockito::Server;
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_initiate_device_auth_success() {
        let mut server = Server::new_async().await;

        let mock = server
            .mock("POST", "/device/code")
            .match_header("accept", "application/json")
            .with_status(200)
            .with_body(
                r#"{
                    "device_code": "test_device_code",
                    "user_code": "ABCD-1234",
                    "verification_uri": "https://github.com/login/device",
                    "expires_in": 900,
                    "interval": 5
                }"#,
            )
            .create_async()
            .await;

        let config = OAuthConfig {
            device_code_url: Some(format!("{}/device/code", server.url())),
            device_token_url: Some(format!("{}/token", server.url())),
            auth_url: None,
            token_url: None,
            client_id: "test-client".to_string(),
            scopes: vec!["read:user".to_string()],
            redirect_uri: String::new(),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
        };

        let service = ForgeOAuthService::new();

        let response = service.initiate_device_auth(&config).await.unwrap();

        assert_eq!(response.device_code, "test_device_code");
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.verification_uri, "https://github.com/login/device");
        assert_eq!(response.expires_in, 900);
        assert_eq!(response.interval, 5);

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_build_auth_code_url_with_pkce() {
        let config = OAuthConfig {
            device_code_url: None,
            device_token_url: None,
            auth_url: Some("https://provider.com/authorize".to_string()),
            token_url: Some("https://provider.com/token".to_string()),
            client_id: "test-client".to_string(),
            scopes: vec!["user:profile".to_string(), "user:email".to_string()],
            redirect_uri: "https://provider.com/callback".to_string(),
            use_pkce: true,
            token_refresh_url: None,
            custom_headers: None,
        };

        let service = ForgeOAuthService::new();

        let params = service.build_auth_code_url(&config).unwrap();

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
            device_code_url: None,
            device_token_url: None,
            auth_url: Some("https://provider.com/authorize".to_string()),
            token_url: Some("https://provider.com/token".to_string()),
            client_id: "test-client".to_string(),
            scopes: vec!["read".to_string()],
            redirect_uri: "https://provider.com/callback".to_string(),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
        };

        let service = ForgeOAuthService::new();

        let params = service.build_auth_code_url(&config).unwrap();

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
            device_code_url: None,
            device_token_url: None,
            auth_url: Some("https://provider.com/auth".to_string()),
            token_url: Some(format!("{}/token", server.url())),
            client_id: "test-client".to_string(),
            scopes: vec![],
            redirect_uri: "https://provider.com/callback".to_string(),
            use_pkce: false,
            token_refresh_url: None,
            custom_headers: None,
        };

        let service = ForgeOAuthService::new();

        let response = service
            .exchange_auth_code(&config, "test_auth_code", None)
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
