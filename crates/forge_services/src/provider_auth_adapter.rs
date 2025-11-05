use forge_domain::OAuthConfig;
use oauth2::{
    AuthorizationCode as OAuth2AuthCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, Scope,
};
use serde::Serialize;

use super::provider_auth_utils::*;

/// Authorization URL parameters
#[derive(Debug, Clone)]
pub struct AuthCodeParams {
    pub auth_url: String,
    pub state: String,
    pub code_verifier: Option<String>,
}

/// Handles provider-specific OAuth quirks
#[async_trait::async_trait]
pub(crate) trait ProviderAdapter: Send + Sync {
    /// Build authorization URL with provider-specific parameters
    async fn build_auth_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams>;

    /// Exchange authorization code with provider-specific handling
    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse>;

    /// Create HTTP client with provider-specific headers/behavior
    fn build_http_client(&self, config: &OAuthConfig) -> anyhow::Result<reqwest::Client> {
        build_http_client(config.custom_headers.as_ref())
    }
}

/// Standard RFC-compliant OAuth provider
pub(crate) struct StandardProvider;

#[async_trait::async_trait]
impl ProviderAdapter for StandardProvider {
    async fn build_auth_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        // Use oauth2 library - standard flow
        use oauth2::{AuthUrl, ClientId, TokenUrl};

        let mut client =
            oauth2::basic::BasicClient::new(ClientId::new(config.client_id.to_string()))
                .set_auth_uri(AuthUrl::new(config.auth_url.to_string())?)
                .set_token_uri(TokenUrl::new(config.token_url.to_string())?);

        if let Some(redirect_uri) = &config.redirect_uri {
            client = client.set_redirect_uri(oauth2::RedirectUrl::new(redirect_uri.clone())?);
        }

        let mut request = client.authorize_url(CsrfToken::new_random);

        for scope in &config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }

        if let Some(extra_params) = &config.extra_auth_params {
            for (key, value) in extra_params {
                request = request.add_extra_param(key, value);
            }
        }

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

    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        use oauth2::{AuthUrl, ClientId, TokenUrl};

        let mut client =
            oauth2::basic::BasicClient::new(ClientId::new(config.client_id.to_string()))
                .set_auth_uri(AuthUrl::new(config.auth_url.to_string())?)
                .set_token_uri(TokenUrl::new(config.token_url.to_string())?);

        if let Some(redirect_uri) = &config.redirect_uri {
            client = client.set_redirect_uri(oauth2::RedirectUrl::new(redirect_uri.clone())?);
        }

        let http_client = self.build_http_client(config)?;

        let mut request = client.exchange_code(OAuth2AuthCode::new(code.to_string()));

        if let Some(v) = verifier {
            request = request.set_pkce_verifier(PkceCodeVerifier::new(v.to_string()));
        }

        let token_result = request.request_async(&http_client).await?;
        Ok(token_result.into())
    }
}

/// Anthropic Provider - Non-standard PKCE implementation
/// Quirk: state parameter equals PKCE verifier
#[allow(unused)]
pub(crate) struct AnthropicProvider;

#[allow(unused)]
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

#[async_trait::async_trait]
impl ProviderAdapter for AnthropicProvider {
    async fn build_auth_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        // Anthropic quirk: state = verifier
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

        let mut url = config.auth_url.clone();
        url.query_pairs_mut()
            .append_pair("client_id", &config.client_id)
            .append_pair("response_type", "code")
            .append_pair("scope", &config.scopes.join(" "))
            .append_pair("code_challenge", challenge.as_str())
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", verifier.secret()); // ← Non-standard!

        if let Some(redirect_uri) = &config.redirect_uri {
            url.query_pairs_mut()
                .append_pair("redirect_uri", redirect_uri);
        }

        if let Some(extra_params) = &config.extra_auth_params {
            for (key, value) in extra_params {
                url.query_pairs_mut().append_pair(key, value);
            }
        }

        Ok(AuthCodeParams {
            auth_url: url.to_string(),
            state: verifier.secret().to_string(),
            code_verifier: Some(verifier.secret().to_string()),
        })
    }

    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Anthropic-specific token exchange
        let (code, state) = if code.contains('#') {
            let parts: Vec<&str> = code.split('#').collect();
            (parts[0].to_string(), parts.get(1).map(|s| s.to_string()))
        } else {
            (code.to_string(), None)
        };

        let verifier = verifier
            .ok_or_else(|| anyhow::anyhow!("PKCE verifier required for Anthropic OAuth"))?;

        let request_body = AnthropicTokenRequest {
            code,
            state: state.unwrap_or_else(|| verifier.to_string()),
            grant_type: "authorization_code".to_string(),
            client_id: config.client_id.to_string(),
            redirect_uri: config.redirect_uri.clone(),
            code_verifier: verifier.to_string(),
        };

        let client = self.build_http_client(config)?;
        let response = client
            .post(config.token_url.as_str())
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
}

/// GitHub Provider - HTTP 200 responses may contain errors
pub(crate) struct GitHubProvider;

#[async_trait::async_trait]
impl ProviderAdapter for GitHubProvider {
    async fn build_auth_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        // Use standard flow - no quirks in auth URL
        StandardProvider.build_auth_url(config).await
    }

    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Use standard exchange - quirks handled in HTTP client via
        // github_compliant_http_request
        StandardProvider.exchange_code(config, code, verifier).await
    }

    fn build_http_client(&self, config: &OAuthConfig) -> anyhow::Result<reqwest::Client> {
        // GitHub quirk: HTTP 200 responses may contain errors
        // This is handled by the github_compliant_http_request function
        build_http_client(config.custom_headers.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::OAuthConfig;
    use url::Url;

    use super::*;

    fn test_oauth_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "test_client".to_string().into(),
            auth_url: Url::parse("https://example.com/auth").unwrap(),
            token_url: Url::parse("https://example.com/token").unwrap(),
            scopes: vec!["read".to_string(), "write".to_string()],
            redirect_uri: Some("https://example.com/callback".to_string()),
            use_pkce: true,
            token_refresh_url: None,
            extra_auth_params: None,
            custom_headers: None,
        }
    }

    #[tokio::test]
    async fn test_standard_provider_build_auth_url() {
        let provider = StandardProvider;
        let config = test_oauth_config();

        let result = provider.build_auth_url(&config).await.unwrap();

        assert!(result.auth_url.contains("client_id=test_client"));
        assert!(result.auth_url.contains("response_type=code"));
        assert!(result.code_verifier.is_some());
        assert_ne!(&result.state, result.code_verifier.as_ref().unwrap());
    }

    #[tokio::test]
    async fn test_anthropic_provider_state_equals_verifier() {
        let provider = AnthropicProvider;
        let config = test_oauth_config();

        let result = provider.build_auth_url(&config).await.unwrap();

        // Anthropic quirk: state should equal verifier
        assert_eq!(result.state, result.code_verifier.unwrap());
        assert!(result.auth_url.contains("code_challenge_method=S256"));
    }
}
