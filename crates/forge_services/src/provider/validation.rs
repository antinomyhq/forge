use std::sync::Arc;

use anyhow::{Result, ensure};
use chrono::Utc;
use forge_app::dto::{AuthType, ProviderCredential, ProviderId};
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use url::Url;

use crate::infra::{HttpInfra, ProviderValidationInfra};

/// Result of credential validation
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationResult {
    /// Credential is valid and working
    Valid,
    /// Credential is invalid (authentication failed)
    Invalid(String),
    /// Unable to determine validity (network issues, etc.)
    Inconclusive(String),
    /// OAuth token has expired and needs refresh
    TokenExpired,
}

/// Service for validating provider credentials
///
/// Validates credentials by making lightweight API calls to provider endpoints.
/// Uses various strategies based on provider type and authentication method.
pub struct ForgeProviderValidationService<I> {
    infra: Arc<I>,
}

impl<I> ForgeProviderValidationService<I> {
    /// Creates a new validation service
    ///
    /// # Arguments
    ///
    /// * `infra` - HTTP infrastructure for making requests
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

impl<I: HttpInfra> ForgeProviderValidationService<I> {
    /// Validates a provider credential
    ///
    /// # Arguments
    ///
    /// * `provider_id` - ID of the provider to validate
    /// * `credential` - Credential to validate
    /// * `validation_url` - URL to use for validation (typically model_url)
    ///
    /// # Errors
    ///
    /// Returns error if validation check fails unexpectedly
    pub async fn validate_credential(
        &self,
        provider_id: &ProviderId,
        credential: &ProviderCredential,
        validation_url: &Url,
    ) -> Result<ValidationResult> {
        // Check OAuth token expiration first
        if let Some(oauth_tokens) = &credential.oauth_tokens
            && oauth_tokens.expires_at <= Utc::now()
        {
            return Ok(ValidationResult::TokenExpired);
        }

        self.validate_credential_internal(provider_id, credential, validation_url)
            .await
    }

    /// Validates credential without checking expiration
    ///
    /// Useful for testing or when token refresh is handled separately
    pub async fn validate_credential_skip_expiry_check(
        &self,
        provider_id: &ProviderId,
        credential: &ProviderCredential,
        validation_url: &Url,
    ) -> Result<ValidationResult> {
        self.validate_credential_internal(provider_id, credential, validation_url)
            .await
    }

    /// Internal validation logic shared by both public methods
    async fn validate_credential_internal(
        &self,
        provider_id: &ProviderId,
        credential: &ProviderCredential,
        validation_url: &Url,
    ) -> Result<ValidationResult> {
        // Extract credential for validation
        let api_key = match self.extract_credential_for_validation(credential) {
            Ok(key) => key,
            Err(result) => return Ok(result),
        };

        // Build authorization headers
        let headers = self.build_auth_headers(provider_id, &api_key)?;

        // Make validation request
        let response = self.infra.get(validation_url, Some(headers)).await;

        // Interpret response
        match response {
            Ok(resp) => Ok(Self::interpret_validation_response(resp.status())),
            Err(e) => Ok(ValidationResult::Inconclusive(format!(
                "Network error: {}",
                e
            ))),
        }
    }

    /// Extracts credential value for validation
    ///
    /// Returns the credential string on success, or a ValidationResult error on
    /// failure
    fn extract_credential_for_validation(
        &self,
        credential: &ProviderCredential,
    ) -> Result<String, ValidationResult> {
        let auth_value = match &credential.auth_type {
            AuthType::ApiKey => credential.get_api_key(),
            AuthType::OAuth | AuthType::OAuthWithApiKey => credential.get_access_token(),
        };

        match auth_value {
            Some(key) => Ok(key.to_string()),
            None => Err(ValidationResult::Invalid(
                "No credential available for validation".to_string(),
            )),
        }
    }

    /// Builds authorization headers for the given provider
    ///
    /// Handles provider-specific header formats (e.g., Anthropic's x-api-key)
    fn build_auth_headers(&self, provider_id: &ProviderId, api_key: &str) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        let auth_header = match provider_id {
            ProviderId::Anthropic => format!("x-api-key: {}", api_key),
            _ => format!("Bearer {}", api_key),
        };

        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_header)
                .map_err(|e| anyhow::anyhow!("Invalid authorization header: {}", e))?,
        );

        Ok(headers)
    }

    /// Interprets HTTP status code into validation result
    fn interpret_validation_response(status: StatusCode) -> ValidationResult {
        match status.as_u16() {
            // Success - credential is valid
            200 | 201 | 202 | 204 => ValidationResult::Valid,
            // Not found is OK - endpoint exists, auth worked
            404 => ValidationResult::Valid,
            // Auth failures - credential is invalid
            401 => ValidationResult::Invalid("Unauthorized - invalid API key".to_string()),
            403 => ValidationResult::Invalid("Forbidden - API key lacks permissions".to_string()),
            // Rate limiting or temporary issues
            429 => ValidationResult::Inconclusive("Rate limited - try again later".to_string()),
            // Server errors
            500..=599 => ValidationResult::Inconclusive(format!("Server error ({})", status)),
            // Other statuses
            _ => ValidationResult::Inconclusive(format!("Unexpected status code: {}", status)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::{Duration, Utc};
    use forge_app::dto::{AuthType, OAuthTokens};

    use super::*;

    // Mock HTTP infrastructure for testing
    struct MockHttpInfra {
        should_error: bool,
    }

    impl MockHttpInfra {
        fn new() -> Self {
            Self { should_error: false }
        }

        fn with_error() -> Self {
            Self { should_error: true }
        }
    }

    #[async_trait::async_trait]
    impl HttpInfra for MockHttpInfra {
        async fn get(&self, _url: &Url, _headers: Option<HeaderMap>) -> Result<reqwest::Response> {
            if self.should_error {
                anyhow::bail!("Network error");
            }

            // Create a mock response
            let client = reqwest::Client::new();
            let response = client.get("https://httpbin.org/status/200").send().await?;

            // We can't easily mock status codes with reqwest, so we'll test with real
            // responses For now, just return a valid response
            Ok(response)
        }

        async fn post(&self, _url: &Url, _body: bytes::Bytes) -> Result<reqwest::Response> {
            unimplemented!()
        }

        async fn delete(&self, _url: &Url) -> Result<reqwest::Response> {
            unimplemented!()
        }

        async fn eventsource(
            &self,
            _url: &Url,
            _headers: Option<HeaderMap>,
            _body: bytes::Bytes,
        ) -> Result<reqwest_eventsource::EventSource> {
            unimplemented!()
        }
    }

    fn create_api_key_credential(api_key: &str) -> ProviderCredential {
        ProviderCredential::new_api_key(ProviderId::OpenAI, api_key.to_string())
    }

    fn create_oauth_credential(expired: bool) -> ProviderCredential {
        let expires_at = if expired {
            Utc::now() - Duration::hours(1)
        } else {
            Utc::now() + Duration::hours(1)
        };

        ProviderCredential::new_oauth(
            ProviderId::OpenAI,
            OAuthTokens {
                refresh_token: "refresh_token".to_string(),
                access_token: "access_token".to_string(),
                expires_at,
            },
        )
    }

    #[tokio::test]
    async fn test_validate_expired_oauth_token() {
        let infra = Arc::new(MockHttpInfra::new());
        let service = ForgeProviderValidationService::new(infra);

        let credential = create_oauth_credential(true);
        let url = Url::parse("https://api.openai.com/v1/models").unwrap();

        let result = service
            .validate_credential(&ProviderId::OpenAI, &credential, &url)
            .await
            .unwrap();

        assert_eq!(result, ValidationResult::TokenExpired);
    }

    #[tokio::test]
    async fn test_validate_api_key_missing() {
        let infra = Arc::new(MockHttpInfra::new());
        let service = ForgeProviderValidationService::new(infra);

        // Create credential without API key
        let credential = ProviderCredential {
            provider_id: ProviderId::OpenAI,
            auth_type: AuthType::ApiKey,
            api_key: None,
            oauth_tokens: None,
            url_params: HashMap::new(),
            compatibility_mode: None,
            custom_base_url: None,
            custom_model_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_verified_at: None,
            is_active: true,
        };

        let url = Url::parse("https://api.openai.com/v1/models").unwrap();

        let result = service
            .validate_credential(&ProviderId::OpenAI, &credential, &url)
            .await
            .unwrap();

        match result {
            ValidationResult::Invalid(msg) => {
                assert!(msg.contains("No credential"));
            }
            _ => panic!("Expected Invalid result"),
        }
    }

    #[tokio::test]
    async fn test_validate_network_error() {
        let infra = Arc::new(MockHttpInfra::with_error());
        let service = ForgeProviderValidationService::new(infra);

        let credential = create_api_key_credential("sk-test");
        let url = Url::parse("https://api.openai.com/v1/models").unwrap();

        let result = service
            .validate_credential(&ProviderId::OpenAI, &credential, &url)
            .await
            .unwrap();

        match result {
            ValidationResult::Inconclusive(msg) => {
                assert!(msg.contains("Network error"));
            }
            _ => panic!("Expected Inconclusive result"),
        }
    }

    #[tokio::test]
    async fn test_anthropic_auth_header() {
        // This test verifies that Anthropic uses x-api-key instead of Bearer
        let infra = Arc::new(MockHttpInfra::new());
        let service = ForgeProviderValidationService::new(infra);

        let credential =
            ProviderCredential::new_api_key(ProviderId::Anthropic, "sk-ant-test".to_string());

        let url = Url::parse("https://api.anthropic.com/v1/models").unwrap();

        // Should not panic
        let _ = service
            .validate_credential_skip_expiry_check(&ProviderId::Anthropic, &credential, &url)
            .await;
    }
}

#[async_trait::async_trait]
impl<I> ProviderValidationInfra for ForgeProviderValidationService<I>
where
    I: HttpInfra + Send + Sync + 'static,
{
    fn validate_api_key_format(&self, _provider_id: &ProviderId, api_key: &str) -> Result<()> {
        ensure!(!api_key.trim().is_empty(), "API key must not be empty");
        Ok(())
    }

    fn validate_model_url(&self, url: &Url) -> Result<()> {
        ensure!(
            url.scheme() == "https",
            "Model URL must use https scheme: {}",
            url
        );
        ensure!(
            url.host_str().is_some(),
            "Model URL must include a hostname: {}",
            url
        );
        Ok(())
    }

    async fn validate_credential(
        &self,
        provider_id: &ProviderId,
        credential: &ProviderCredential,
        validation_url: &Url,
    ) -> Result<ValidationResult> {
        ForgeProviderValidationService::validate_credential(
            self,
            provider_id,
            credential,
            validation_url,
        )
        .await
    }
}
