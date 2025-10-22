/// API key authentication flow for simple providers and cloud services
use std::collections::HashMap;
use std::time::Duration;

use forge_app::dto::{
    AuthContext, AuthInitiation, AuthMethod, AuthResult, ProviderCredential, ProviderId,
    UrlParameter,
};
use regex::Regex;

use super::{AuthFlowError, AuthenticationFlow};

/// API key authentication flow.
/// Used by providers that require an API key, with optional URL parameters:
pub struct ApiKeyAuthFlow {
    provider_id: ProviderId,
    required_params: Vec<UrlParameter>,
}

impl ApiKeyAuthFlow {
    /// Creates a new API key authentication flow without URL parameters.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - The provider requiring authentication
    pub fn new(provider_id: ProviderId) -> Self {
        Self { provider_id, required_params: Vec::new() }
    }

    /// Creates a new API key authentication flow with URL parameters.
    pub fn with_params(provider_id: ProviderId, required_params: Vec<UrlParameter>) -> Self {
        Self { provider_id, required_params }
    }

    /// Creates Vertex AI configuration
    pub fn vertex_ai(provider_id: ProviderId) -> Self {
        let params = vec![
            UrlParameter::required("project_id", "GCP Project ID")
                .description("Your Google Cloud project ID")
                .validation_pattern(r"^[a-z][a-z0-9-]{4,28}[a-z0-9]$"),
            UrlParameter::required("location", "Location")
                .description("GCP region (e.g., us-central1) or 'global'")
                .default_value("us-central1"),
        ];

        Self::with_params(provider_id, params)
    }

    /// Creates Azure OpenAI configuration
    pub fn azure_openai(provider_id: ProviderId) -> Self {
        let params = vec![
            UrlParameter::required("resource_name", "Azure Resource Name")
                .description("Your Azure OpenAI resource name"),
            UrlParameter::required("deployment_name", "Deployment Name")
                .description("Your model deployment name"),
            UrlParameter::required("api_version", "API Version")
                .description("Azure API version")
                .default_value("2024-02-15-preview"),
        ];

        Self::with_params(provider_id, params)
    }

    /// Validates a parameter value against its validation pattern
    fn validate_parameter(&self, param: &UrlParameter, value: &str) -> Result<(), AuthFlowError> {
        // Check if empty when required
        if param.required && value.trim().is_empty() {
            return Err(AuthFlowError::MissingParameter(param.key.clone()));
        }

        // Validate against regex pattern if provided
        if let Some(pattern) = &param.validation_pattern {
            let regex = Regex::new(pattern).map_err(|e| {
                AuthFlowError::InvalidParameter(
                    param.key.clone(),
                    format!("Invalid validation pattern: {}", e),
                )
            })?;

            if !regex.is_match(value) {
                return Err(AuthFlowError::InvalidParameter(
                    param.key.clone(),
                    format!("Value '{}' does not match required pattern", value),
                ));
            }
        }

        Ok(())
    }

    /// Validates all required parameters are present and valid
    fn validate_all_parameters(
        &self,
        url_params: &HashMap<String, String>,
    ) -> Result<(), AuthFlowError> {
        for param in &self.required_params {
            if param.required {
                let value = url_params
                    .get(&param.key)
                    .ok_or_else(|| AuthFlowError::MissingParameter(param.key.clone()))?;

                self.validate_parameter(param, value)?;
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AuthenticationFlow for ApiKeyAuthFlow {
    fn auth_method_type(&self) -> AuthMethod {
        AuthMethod::ApiKey
    }

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
                if !self.required_params.is_empty() {
                    self.validate_all_parameters(&url_params)?;

                    // Create credential with API key and URL parameters
                    let mut credential =
                        ProviderCredential::new_api_key(self.provider_id.clone(), api_key);
                    credential.url_params = url_params;

                    Ok(credential)
                } else {
                    // Simple API key providers should not have URL parameters
                    if !url_params.is_empty() {
                        return Err(AuthFlowError::CompletionFailed(
                            "Simple API key providers should not have URL parameters".to_string(),
                        ));
                    }

                    Ok(ProviderCredential::new_api_key(
                        self.provider_id.clone(),
                        api_key,
                    ))
                }
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
        ApiKeyAuthFlow::new(ProviderId::OpenAI)
    }

    fn vertex_ai_fixture() -> ApiKeyAuthFlow {
        ApiKeyAuthFlow::vertex_ai(ProviderId::VertexAi)
    }

    fn azure_fixture() -> ApiKeyAuthFlow {
        ApiKeyAuthFlow::azure_openai(ProviderId::Azure)
    }

    #[test]
    fn test_auth_method_type() {
        let flow = create_flow();
        assert_eq!(flow.auth_method_type(), AuthMethod::ApiKey);
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

    // Tests for cloud service providers (Vertex AI, Azure)

    #[tokio::test]
    async fn test_initiate_vertex_ai() {
        let flow = vertex_ai_fixture();
        let result = flow.initiate().await.unwrap();

        match result {
            AuthInitiation::ApiKeyPrompt { required_params, .. } => {
                assert_eq!(required_params.len(), 2);
                assert_eq!(required_params[0].key, "project_id");
                assert_eq!(required_params[1].key, "location");
                assert!(required_params[0].required);
                assert!(required_params[1].required);
            }
            _ => panic!("Expected ApiKeyPrompt"),
        }
    }

    #[tokio::test]
    async fn test_initiate_azure() {
        let flow = azure_fixture();
        let result = flow.initiate().await.unwrap();

        match result {
            AuthInitiation::ApiKeyPrompt { required_params, .. } => {
                assert_eq!(required_params.len(), 3);
                assert_eq!(required_params[0].key, "resource_name");
                assert_eq!(required_params[1].key, "deployment_name");
                assert_eq!(required_params[2].key, "api_version");
            }
            _ => panic!("Expected ApiKeyPrompt"),
        }
    }

    #[tokio::test]
    async fn test_complete_with_valid_params() {
        let flow = vertex_ai_fixture();

        let mut url_params = HashMap::new();
        url_params.insert("project_id".to_string(), "my-project-123".to_string());
        url_params.insert("location".to_string(), "us-central1".to_string());

        let auth_result = AuthResult::ApiKey { api_key: "test-api-key".to_string(), url_params };

        let credential = flow.complete(auth_result).await.unwrap();

        assert_eq!(credential.provider_id, ProviderId::VertexAi);
        assert_eq!(credential.get_api_key(), Some("test-api-key"));
        assert_eq!(
            credential.url_params.get("project_id"),
            Some(&"my-project-123".to_string())
        );
        assert_eq!(
            credential.url_params.get("location"),
            Some(&"us-central1".to_string())
        );
    }

    #[tokio::test]
    async fn test_complete_with_missing_required_param() {
        let flow = vertex_ai_fixture();

        let mut url_params = HashMap::new();
        url_params.insert("location".to_string(), "us-central1".to_string());
        // Missing project_id

        let auth_result = AuthResult::ApiKey { api_key: "test-api-key".to_string(), url_params };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("project_id"));
    }

    #[tokio::test]
    async fn test_complete_with_invalid_project_id_pattern() {
        let flow = vertex_ai_fixture();

        let mut url_params = HashMap::new();
        url_params.insert("project_id".to_string(), "Invalid_Project!".to_string());
        url_params.insert("location".to_string(), "us-central1".to_string());

        let auth_result = AuthResult::ApiKey { api_key: "test-api-key".to_string(), url_params };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("project_id"));
        assert!(err.contains("does not match"));
    }

    #[tokio::test]
    async fn test_vertex_ai_complete_with_empty_api_key() {
        let flow = vertex_ai_fixture();

        let mut url_params = HashMap::new();
        url_params.insert("project_id".to_string(), "my-project-123".to_string());
        url_params.insert("location".to_string(), "us-central1".to_string());

        let auth_result = AuthResult::ApiKey {
            api_key: "   ".to_string(), // Empty/whitespace
            url_params,
        };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("API key cannot be empty"));
    }

    #[tokio::test]
    async fn test_azure_complete_with_all_params() {
        let flow = azure_fixture();

        let mut url_params = HashMap::new();
        url_params.insert("resource_name".to_string(), "my-resource".to_string());
        url_params.insert(
            "deployment_name".to_string(),
            "gpt-4-deployment".to_string(),
        );
        url_params.insert("api_version".to_string(), "2024-02-15-preview".to_string());

        let auth_result = AuthResult::ApiKey { api_key: "azure-api-key".to_string(), url_params };

        let credential = flow.complete(auth_result).await.unwrap();

        assert_eq!(credential.provider_id, ProviderId::Azure);
        assert_eq!(credential.get_api_key(), Some("azure-api-key"));
        assert_eq!(credential.url_params.len(), 3);
    }
}
