use forge_app::OAuthHttpProvider;
use forge_domain::{AuthCodeParams, OAuthConfig, OAuthTokenResponse};
use oauth2::{
    AuthorizationCode as OAuth2AuthCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, Scope,
};

use crate::auth::util::*;

/// Standard RFC-compliant OAuth provider
pub struct StandardHttpProvider;

#[async_trait::async_trait]
impl OAuthHttpProvider for StandardHttpProvider {
    async fn build_auth_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        // Use oauth2 library - standard flow
        use oauth2::{AuthUrl, ClientId, TokenUrl};

        let client_id = config
            .client_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("client_id is required for OAuth code flow"))?
            .to_string();

        let mut client = oauth2::basic::BasicClient::new(ClientId::new(client_id))
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

        let client_id = config
            .client_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("client_id is required for OAuth code flow"))?
            .to_string();

        let mut client = oauth2::basic::BasicClient::new(ClientId::new(client_id))
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
        Ok(into_domain(token_result))
    }

    /// Create HTTP client with provider-specific headers/behavior
    fn build_http_client(&self, config: &OAuthConfig) -> anyhow::Result<reqwest::Client> {
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
            client_id: Some("test_client".to_string().into()),
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
        let provider = StandardHttpProvider;
        let config = test_oauth_config();

        let result = provider.build_auth_url(&config).await.unwrap();

        assert!(result.auth_url.contains("client_id=test_client"));
        assert!(result.auth_url.contains("response_type=code"));
        assert!(result.code_verifier.is_some());
        assert_ne!(&result.state, result.code_verifier.as_ref().unwrap());
    }
}
