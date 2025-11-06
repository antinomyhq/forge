use forge_app::OAuthHttpProvider;
use forge_domain::{AuthCodeParams, OAuthConfig, OAuthTokenResponse};

use super::standard::StandardHttpProvider;
use crate::auth::util::build_http_client;

/// GitHub Provider - HTTP 200 responses may contain errors
pub struct GithubHttpProvider;

#[async_trait::async_trait]
impl OAuthHttpProvider for GithubHttpProvider {
    async fn build_auth_url(&self, config: &OAuthConfig) -> anyhow::Result<AuthCodeParams> {
        // Use standard flow - no quirks in auth URL
        StandardHttpProvider.build_auth_url(config).await
    }

    async fn exchange_code(
        &self,
        config: &OAuthConfig,
        code: &str,
        verifier: Option<&str>,
    ) -> anyhow::Result<OAuthTokenResponse> {
        // Use standard exchange - quirks handled in HTTP client via
        // github_compliant_http_request
        StandardHttpProvider
            .exchange_code(config, code, verifier)
            .await
    }

    fn build_http_client(&self, config: &OAuthConfig) -> anyhow::Result<reqwest::Client> {
        // GitHub quirk: HTTP 200 responses may contain errors
        // This is handled by the github_compliant_http_request function
        build_http_client(config.custom_headers.as_ref())
    }
}
