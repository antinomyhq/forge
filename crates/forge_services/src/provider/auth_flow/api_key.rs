/// API key authentication flow for simple providers (OpenAI, Anthropic, etc.)
use std::time::Duration;

use forge_app::dto::{
    AuthContext, AuthInitiation, AuthMethodType, AuthResult, ProviderCredential, ProviderId,
};

use super::{AuthFlowError, AuthenticationFlow};

/// Simple API key authentication flow.
///
/// Used by providers that only require a static API key:
/// - OpenAI
/// - Anthropic
/// - OpenRouter
/// - Cerebras
/// - xAI
/// - BigModel
///
/// This flow prompts the user for an API key and creates a credential
/// immediately. It does not support polling (user must manually provide the
/// key) or refresh.
pub struct ApiKeyAuthFlow {
    provider_id: ProviderId,
    label: String,
    description: Option<String>,
}

impl ApiKeyAuthFlow {
    /// Creates a new API key authentication flow.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - The provider requiring authentication
    /// * `label` - Display label for the API key prompt (e.g., "OpenAI API
    ///   Key")
    /// * `description` - Optional description explaining where to get the key
    pub fn new(
        provider_id: ProviderId,
        label: impl Into<String>,
        description: Option<String>,
    ) -> Self {
        Self { provider_id, label: label.into(), description }
    }
}

#[async_trait::async_trait]
impl AuthenticationFlow for ApiKeyAuthFlow {
    fn auth_method_type(&self) -> AuthMethodType {
        AuthMethodType::ApiKey
    }

    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        Ok(AuthInitiation::ApiKeyPrompt {
            label: self.label.clone(),
            description: self.description.clone(),
            required_params: Vec::new(), // Simple API key providers don't need additional params
        })
    }

    async fn poll_until_complete(
        &self,
        _context: &AuthContext,
        _timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError> {
        // API key flows are not pollable - the user must manually provide the key
        Err(AuthFlowError::PollFailed(
            "API key authentication requires manual input and cannot be polled".to_string(),
        ))
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match result {
            AuthResult::ApiKey { api_key, url_params } => {
                // Simple API key providers should not have URL parameters
                if !url_params.is_empty() {
                    return Err(AuthFlowError::CompletionFailed(
                        "Simple API key providers should not have URL parameters".to_string(),
                    ));
                }

                // Validate API key is not empty
                if api_key.trim().is_empty() {
                    return Err(AuthFlowError::CompletionFailed(
                        "API key cannot be empty".to_string(),
                    ));
                }

                Ok(ProviderCredential::new_api_key(
                    self.provider_id.clone(),
                    api_key,
                ))
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "Expected API key result".to_string(),
            )),
        }
    }

    async fn refresh(
        &self,
        _credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        // Static API keys don't support refresh
        Err(AuthFlowError::RefreshFailed(
            "API key credentials cannot be refreshed".to_string(),
        ))
    }

    async fn validate(&self, _credential: &ProviderCredential) -> Result<bool, AuthFlowError> {
        // Assume static API keys are always valid
        // Actual validation would require making an API call to the provider
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use super::*;

    fn create_flow() -> ApiKeyAuthFlow {
        ApiKeyAuthFlow::new(
            ProviderId::OpenAI,
            "OpenAI API Key",
            Some("Get your API key from https://platform.openai.com/api-keys".to_string()),
        )
    }

    #[test]
    fn test_auth_method_type() {
        let flow = create_flow();
        assert_eq!(flow.auth_method_type(), AuthMethodType::ApiKey);
    }

    #[tokio::test]
    async fn test_initiate() {
        let flow = create_flow();
        let result = flow.initiate().await.unwrap();

        match result {
            AuthInitiation::ApiKeyPrompt { label, description, required_params } => {
                assert_eq!(label, "OpenAI API Key");
                assert_eq!(
                    description,
                    Some("Get your API key from https://platform.openai.com/api-keys".to_string())
                );
                assert!(required_params.is_empty());
            }
            _ => panic!("Expected ApiKeyPrompt"),
        }
    }

    #[tokio::test]
    async fn test_poll_returns_error() {
        let flow = create_flow();
        let context = AuthContext::new();
        let result = flow
            .poll_until_complete(&context, Duration::from_secs(60))
            .await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, AuthFlowError::PollFailed(_)));
    }

    #[tokio::test]
    async fn test_complete_success() {
        let flow = create_flow();
        let result = AuthResult::ApiKey {
            api_key: "sk-test123".to_string(),
            url_params: HashMap::new(),
        };

        let credential = flow.complete(result).await.unwrap();

        assert_eq!(credential.provider_id, ProviderId::OpenAI);
        assert_eq!(credential.get_api_key(), Some("sk-test123"));
    }

    #[tokio::test]
    async fn test_complete_with_url_params_fails() {
        let flow = create_flow();
        let mut url_params = HashMap::new();
        url_params.insert("project_id".to_string(), "test".to_string());

        let result = AuthResult::ApiKey { api_key: "sk-test123".to_string(), url_params };

        let error = flow.complete(result).await.unwrap_err();
        assert!(matches!(error, AuthFlowError::CompletionFailed(_)));
    }

    #[tokio::test]
    async fn test_complete_with_empty_key_fails() {
        let flow = create_flow();
        let result = AuthResult::ApiKey { api_key: "   ".to_string(), url_params: HashMap::new() };

        let error = flow.complete(result).await.unwrap_err();
        assert!(matches!(error, AuthFlowError::CompletionFailed(_)));
    }

    #[tokio::test]
    async fn test_complete_wrong_result_type() {
        let flow = create_flow();
        let result = AuthResult::OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: None,
            expires_in: None,
        };

        let error = flow.complete(result).await.unwrap_err();
        assert!(matches!(error, AuthFlowError::CompletionFailed(_)));
    }

    #[tokio::test]
    async fn test_refresh_returns_error() {
        let flow = create_flow();
        let credential = ProviderCredential::new_api_key(ProviderId::OpenAI, "sk-test".to_string());

        let result = flow.refresh(&credential).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(matches!(error, AuthFlowError::RefreshFailed(_)));
    }

    #[tokio::test]
    async fn test_validate_always_returns_true() {
        let flow = create_flow();
        let credential = ProviderCredential::new_api_key(ProviderId::OpenAI, "sk-test".to_string());

        let is_valid = flow.validate(&credential).await.unwrap();
        assert!(is_valid);
    }
}
