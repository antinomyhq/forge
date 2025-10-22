/// API key authentication flow for simple providers and cloud services
use std::time::Duration;

use forge_app::dto::{
    AuthContext, AuthInitiation, AuthResult, ProviderCredential, ProviderId, URLParam,
};

use super::{AuthFlowError, AuthenticationFlow};

/// API key authentication flow.
/// Used by providers that require an API key, with optional URL parameters:
pub struct ApiKeyAuthFlow {
    provider_id: ProviderId,
    required_params: Vec<URLParam>,
}

impl ApiKeyAuthFlow {
    /// Creates a new API key authentication flow with URL parameters.
    pub fn new(provider_id: ProviderId, required_params: Vec<URLParam>) -> Self {
        Self { provider_id, required_params }
    }
}

#[async_trait::async_trait]
impl AuthenticationFlow for ApiKeyAuthFlow {
    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        Ok(AuthInitiation::ApiKeyPrompt { required_params: self.required_params.clone() })
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
                // Validate API key is not empty
                if api_key.trim().is_empty() {
                    return Err(AuthFlowError::CompletionFailed(
                        "API key cannot be empty".to_string(),
                    ));
                }

                // Validate URL parameters if required

                // Create credential with API key and URL parameters
                let mut credential =
                    ProviderCredential::new_api_key(self.provider_id.clone(), api_key);
                credential.url_params = url_params;

                Ok(credential)
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
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use super::*;

    fn create_flow() -> ApiKeyAuthFlow {
        ApiKeyAuthFlow::new(ProviderId::OpenAI, vec![])
    }

    #[tokio::test]
    async fn test_initiate() {
        let flow = create_flow();
        let result = flow.initiate().await.unwrap();

        match result {
            AuthInitiation::ApiKeyPrompt { required_params } => {
                assert!(required_params.is_empty());
            }
            _ => panic!("Expected ApiKeyPrompt"),
        }
    }

    #[tokio::test]
    async fn test_poll_returns_error() {
        let flow = create_flow();
        let context = AuthContext::default();
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
}
