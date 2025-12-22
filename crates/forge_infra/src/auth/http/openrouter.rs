use forge_app::OAuthHttpProvider;
use forge_domain::{AuthCodeParams, OAuthConfig, OAuthTokenResponse};
use oauth2::PkceCodeChallenge;
use serde::{Deserialize, Serialize};

use crate::auth::util::build_http_client;

/// OpenRouter Provider - Simplified PKCE flow without client_id
/// OpenRouter uses a unique flow where no OAuth client registration is needed
pub struct OpenRouterHttpProvider;

#[derive(Debug, Serialize)]
struct OpenRouterTokenRequest {
    code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_verifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_challenge_method: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterTokenResponse {
    key: String,
}

#[async_trait::async_trait]
impl OAuthHttpProvider for OpenRouterHttpProvider {
    async fn build_auth_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        // OpenRouter PKCE flow - no client_id needed
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

        // OpenRouter requires callback_url
        let callback_url = config.redirect_uri.as_ref().ok_or_else(|| {
            anyhow::anyhow!("redirect_uri is required for OpenRouter OAuth (used as callback_url)")
        })?;

        let mut url = config.auth_url.clone();

        // Add callback_url (redirect_uri in OpenRouter's terms)
        url.query_pairs_mut()
            .append_pair("callback_url", callback_url);

        // Add PKCE parameters
        url.query_pairs_mut()
            .append_pair("code_challenge", challenge.as_str())
            .append_pair("code_challenge_method", "S256");

        // Add any extra auth params
        if let Some(extra_params) = &config.extra_auth_params {
            for (key, value) in extra_params {
                url.query_pairs_mut().append_pair(key, value);
            }
        }

        // Use a random state for CSRF protection
        let state = oauth2::CsrfToken::new_random().secret().to_string();

        Ok(AuthCodeParams {
            auth_url: url.to_string(),
            state,
            code_verifier: Some(verifier.secret().to_string()),
        })
    }

    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        let verifier = verifier
            .ok_or_else(|| anyhow::anyhow!("PKCE verifier required for OpenRouter OAuth"))?;

        let request_body = OpenRouterTokenRequest {
            code: code.to_string(),
            code_verifier: Some(verifier.to_string()),
            code_challenge_method: Some("S256".to_string()),
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
            anyhow::bail!("OpenRouter token exchange failed with status {status}: {error_text}");
        }

        let token_response: OpenRouterTokenResponse = response.json().await?;

        // OpenRouter returns an API key directly, not OAuth tokens
        // API keys from OpenRouter don't have expiration, so expires_at is None
        Ok(OAuthTokenResponse {
            access_token: token_response.key,
            refresh_token: None,
            expires_in: None,
            expires_at: None, // OpenRouter API keys don't expire
            token_type: "Bearer".to_string(),
            scope: None,
        })
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
            client_id: None, // OpenRouter doesn't need client_id
            auth_url: Url::parse("https://openrouter.ai/auth").unwrap(),
            token_url: Url::parse("https://openrouter.ai/api/v1/auth/keys").unwrap(),
            scopes: vec![],
            redirect_uri: Some("http://localhost:3000/callback".to_string()),
            use_pkce: true,
            token_refresh_url: None,
            extra_auth_params: None,
            custom_headers: None,
        }
    }

    #[tokio::test]
    async fn test_openrouter_provider_build_auth_url() {
        let provider = OpenRouterHttpProvider;
        let config = test_oauth_config();

        let result = provider.build_auth_url(&config).await.unwrap();

        assert!(
            result
                .auth_url
                .contains("callback_url=http%3A%2F%2Flocalhost%3A3000%2Fcallback")
        );
        assert!(result.auth_url.contains("code_challenge_method=S256"));
        assert!(result.auth_url.contains("code_challenge="));
        assert!(result.code_verifier.is_some());
        // OpenRouter doesn't include client_id in URL
        assert!(!result.auth_url.contains("client_id"));
    }
}
