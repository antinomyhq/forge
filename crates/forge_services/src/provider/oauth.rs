/// OAuth flow implementation for provider authentication
///
/// Supports both device authorization flow (GitHub Copilot) and authorization
/// code flow with manual paste (Anthropic). No local server required.
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::provider::{
    OAuthConfig, generate_code_challenge, generate_code_verifier, generate_state,
};

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

/// OAuth service for handling device and code flows
pub struct ForgeOAuthService {
    client: reqwest::Client,
}

impl ForgeOAuthService {
    /// Creates a new OAuth service
    pub fn new() -> Self {
        Self { client: reqwest::Client::new() }
    }

    /// Creates OAuth service with custom HTTP client
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for ForgeOAuthService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeOAuthService {
    /// Initiates device authorization flow (GitHub Copilot pattern)
    ///
    /// # Arguments
    ///
    /// * `config` - OAuth configuration with device_code_url and client_id
    ///
    /// # Returns
    ///
    /// Device authorization response with user_code and verification_uri
    ///
    /// # Errors
    ///
    /// Returns error if HTTP request fails or response is invalid
    pub async fn initiate_device_auth(
        &self,
        config: &OAuthConfig,
    ) -> anyhow::Result<DeviceAuthorizationResponse> {
        let device_code_url = config
            .device_code_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("device_code_url not configured"))?;

        // Build form params
        let scopes = config.scopes.join(" ");
        let params = vec![("client_id", config.client_id.as_str()), ("scope", &scopes)];

        // GitHub requires specific headers like opencode
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::ACCEPT, "application/json".parse().unwrap());
        headers.insert(
            reqwest::header::USER_AGENT,
            "GitHubCopilotChat/0.26.7".parse().unwrap(),
        );

        let response = self
            .client
            .post(device_code_url)
            .headers(headers)
            .form(&params)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Device authorization failed ({}): {}", status, body);
        }

        let device_response: DeviceAuthorizationResponse = response.json().await?;
        Ok(device_response)
    }

    /// Polls for device authorization completion
    ///
    /// Polls the token endpoint until user completes authorization or timeout.
    /// Implements exponential backoff for rate limit errors.
    ///
    /// # Arguments
    ///
    /// * `config` - OAuth configuration with device_token_url
    /// * `device_code` - Device code from initiation response
    /// * `interval` - Base polling interval in seconds
    ///
    /// # Returns
    ///
    /// OAuth tokens once user authorizes
    ///
    /// # Errors
    ///
    /// Returns error if authorization fails, expires, or times out
    pub async fn poll_device_auth(
        &self,
        config: &OAuthConfig,
        device_code: &str,
        interval: u64,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let token_url = config
            .device_token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("device_token_url not configured"))?;

        let mut current_interval = Duration::from_secs(interval);
        let max_attempts = 100; // Prevents infinite loop
        let mut attempts = 0;

        loop {
            if attempts >= max_attempts {
                anyhow::bail!(
                    "Device authorization timed out after {} attempts",
                    max_attempts
                );
            }
            attempts += 1;

            sleep(current_interval).await;

            let params = vec![
                ("client_id", config.client_id.as_str()),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ];

            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(reqwest::header::ACCEPT, "application/json".parse().unwrap());
            headers.insert(
                reqwest::header::USER_AGENT,
                "GitHubCopilotChat/0.26.7".parse().unwrap(),
            );

            let response = self
                .client
                .post(token_url)
                .headers(headers)
                .form(&params)
                .send()
                .await?;

            let status = response.status();
            let body = response.text().await?;

            // GitHub returns 200 OK even for pending/error states
            // Check for error field first
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(error) = json.get("error").and_then(|e| e.as_str()) {
                    match error {
                        "authorization_pending" => {
                            // User hasn't authorized yet, keep polling
                            continue;
                        }
                        "slow_down" => {
                            // Rate limited, increase interval by 5 seconds
                            current_interval += Duration::from_secs(5);
                            continue;
                        }
                        "expired_token" => {
                            anyhow::bail!("Device code expired. Please restart authorization.");
                        }
                        "access_denied" => {
                            anyhow::bail!("User denied authorization.");
                        }
                        _ => {
                            anyhow::bail!("Authorization failed: {}", error);
                        }
                    }
                }

                // No error field, try to parse as token response
                if status.is_success() {
                    let token_response: OAuthTokenResponse = serde_json::from_str(&body)?;
                    return Ok(token_response);
                }
            }

            // Unknown error format
            anyhow::bail!("Device authorization error: {}", body);
        }
    }

    /// Builds authorization URL for code flow (Anthropic pattern)
    ///
    /// Generates URL with state and optionally PKCE parameters. User visits
    /// URL, authorizes, and manually copies code from provider's callback page.
    ///
    /// # Arguments
    ///
    /// * `config` - OAuth configuration with auth_url, client_id, etc.
    ///
    /// # Returns
    ///
    /// Tuple of (authorization_url, state, code_verifier)
    /// Store state and code_verifier securely for validation/exchange
    ///
    /// # Errors
    ///
    /// Returns error if URL building or PKCE generation fails
    pub fn build_auth_code_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        let auth_url = config
            .auth_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("auth_url not configured"))?;

        let state = generate_state();
        let code_verifier = if config.use_pkce {
            Some(generate_code_verifier())
        } else {
            None
        };

        let mut url = reqwest::Url::parse(auth_url)?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("client_id", &config.client_id);
            query.append_pair("redirect_uri", &config.redirect_uri);
            query.append_pair("response_type", "code");
            query.append_pair("scope", &config.scopes.join(" "));
            query.append_pair("state", &state);

            if let Some(verifier) = &code_verifier {
                let challenge = generate_code_challenge(verifier)?;
                query.append_pair("code_challenge", &challenge);
                query.append_pair("code_challenge_method", "S256");
            }
        }

        Ok(AuthCodeParams { auth_url: url.to_string(), state, code_verifier })
    }

    /// Exchanges authorization code for tokens
    ///
    /// Called after user pastes authorization code from provider's callback
    /// page.
    ///
    /// # Arguments
    ///
    /// * `config` - OAuth configuration with token_url
    /// * `auth_code` - Authorization code from user
    /// * `code_verifier` - PKCE verifier (if PKCE was used)
    ///
    /// # Returns
    ///
    /// OAuth tokens (access_token, refresh_token, etc.)
    ///
    /// # Errors
    ///
    /// Returns error if HTTP request fails or code is invalid
    pub async fn exchange_auth_code(
        &self,
        config: &OAuthConfig,
        auth_code: &str,
        code_verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let token_url = config
            .token_url
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("token_url not configured"))?;

        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("code", auth_code),
            ("client_id", &config.client_id),
            ("redirect_uri", &config.redirect_uri),
        ];

        // Add PKCE verifier if provided
        let verifier_owned: String;
        if let Some(verifier) = code_verifier {
            verifier_owned = verifier.to_string();
            params.push(("code_verifier", &verifier_owned));
        }

        let response = self.client.post(token_url).form(&params).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Token exchange failed ({}): {}", status, body);
        }

        let token_response: OAuthTokenResponse = response.json().await?;
        Ok(token_response)
    }

    /// Refreshes access token using refresh token
    ///
    /// # Arguments
    ///
    /// * `config` - OAuth configuration with token_url
    /// * `refresh_token` - Refresh token from previous authorization
    ///
    /// # Returns
    ///
    /// New OAuth tokens
    ///
    /// # Errors
    ///
    /// Returns error if refresh token is invalid or expired
    pub async fn refresh_access_token(
        &self,
        config: &OAuthConfig,
        refresh_token: &str,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let token_url = config
            .token_url
            .as_ref()
            .or(config.device_token_url.as_ref())
            .ok_or_else(|| anyhow::anyhow!("token_url not configured"))?;

        let params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &config.client_id),
        ];

        let response = self.client.post(token_url).form(&params).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Token refresh failed ({}): {}", status, body);
        }

        let token_response: OAuthTokenResponse = response.json().await?;
        Ok(token_response)
    }

    /// Fetches GitHub Copilot API key from OAuth token
    ///
    /// GitHub Copilot specific: Uses OAuth token to fetch time-limited API key.
    /// The API key is what's actually used for Copilot API requests.
    ///
    /// # Arguments
    ///
    /// * `github_token` - GitHub OAuth access token from device flow
    ///
    /// # Returns
    ///
    /// Tuple of (api_key, expires_at)
    ///
    /// # Errors
    ///
    /// Returns error if user doesn't have Copilot access or request fails
    pub async fn get_copilot_api_key(
        &self,
        github_token: &str,
    ) -> anyhow::Result<(String, DateTime<Utc>)> {
        let url = "https://api.github.com/copilot_internal/v2/token";

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", github_token).parse().unwrap(),
        );
        headers.insert(reqwest::header::ACCEPT, "application/json".parse().unwrap());
        headers.insert(
            reqwest::header::USER_AGENT,
            "GitHubCopilotChat/0.26.7".parse().unwrap(),
        );
        // Add editor headers like opencode does
        headers.insert(
            reqwest::header::HeaderName::from_static("editor-version"),
            "vscode/1.99.3".parse().unwrap(),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("editor-plugin-version"),
            "copilot-chat/0.26.7".parse().unwrap(),
        );

        let response = self.client.get(url).headers(headers).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();

            if status.as_u16() == 403 {
                anyhow::bail!(
                    "GitHub Copilot access denied. Ensure you have an active Copilot subscription."
                );
            }

            anyhow::bail!("Copilot API key fetch failed ({}): {}", status, body);
        }

        #[derive(Deserialize)]
        struct CopilotTokenResponse {
            token: String,
            expires_at: i64,
            #[serde(default)]
            #[allow(dead_code)]
            refresh_in: Option<i64>,
        }

        let copilot_response: CopilotTokenResponse = response.json().await?;

        let expires_at =
            DateTime::from_timestamp(copilot_response.expires_at, 0).unwrap_or_else(Utc::now);

        Ok((copilot_response.token, expires_at))
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
            device_token_url: None,
            auth_url: None,
            token_url: None,
            client_id: "test-client".to_string(),
            scopes: vec!["read:user".to_string()],
            redirect_uri: String::new(),
            use_pkce: false,
            token_refresh_url: None,
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
        assert_eq!(params.state.len(), 64); // 32 bytes hex = 64 chars
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
