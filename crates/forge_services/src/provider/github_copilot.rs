/// GitHub Copilot-specific authentication logic
///
/// GitHub Copilot requires exchanging OAuth tokens for time-limited API keys.
/// This module handles that provider-specific flow.
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Service for GitHub Copilot-specific authentication operations
pub struct GitHubCopilotService {
    client: reqwest::Client,
}

impl Default for GitHubCopilotService {
    fn default() -> Self {
        Self { client: reqwest::Client::new() }
    }
}

impl GitHubCopilotService {
    /// Fetches GitHub Copilot API key from OAuth token
    ///
    /// GitHub Copilot specific: Uses OAuth token to fetch time-limited API key.
    /// The API key is what's actually used for Copilot API requests.
    ///
    /// # Arguments
    /// * `github_token` - GitHub OAuth access token from device flow
    ///
    /// # Returns
    /// Tuple of (api_key, expires_at)
    ///
    /// # Errors
    /// Returns error if user doesn't have Copilot access or request fails
    pub async fn get_copilot_api_key(
        &self,
        github_token: &str,
    ) -> anyhow::Result<(String, DateTime<Utc>)> {
        let url = "https://api.github.com/copilot_internal/v2/token";

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", github_token))
                .expect("Invalid authorization header value"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("GitHubCopilotChat/0.26.7"),
        );
        // Add editor headers like opencode does
        headers.insert(
            reqwest::header::HeaderName::from_static("editor-version"),
            reqwest::header::HeaderValue::from_static("vscode/1.99.3"),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("editor-plugin-version"),
            reqwest::header::HeaderValue::from_static("copilot-chat/0.26.7"),
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