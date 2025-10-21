//! Cloud Service Authentication Flow
//!
//! Implements authentication for cloud providers that require API keys plus
//! additional URL parameters (project IDs, resource names, locations, etc.).
//!
//! ## Supported Providers
//!
//! - **Google Vertex AI**: Requires project_id and location parameters
//! - **Azure OpenAI**: Requires resource_name, deployment_name, and api_version
//!
//! ## Flow
//!
//! 1. **Initiate**: Returns API key prompt with provider-specific parameters
//! 2. **Poll**: Not applicable (manual input required)
//! 3. **Complete**: Validates parameters and creates credential with url_params
//! 4. **Refresh**: Not supported (cloud tokens managed externally)
//! 5. **Validate**: Always returns true (assumes externally managed)

use std::collections::HashMap;
use std::time::Duration;

use forge_app::dto::{
    AuthContext, AuthInitiation, AuthMethodType, AuthResult, ProviderCredential, ProviderId,
    UrlParameter,
};
use regex::Regex;

use crate::provider::auth_flow::AuthenticationFlow;
use crate::provider::auth_flow::error::AuthFlowError;

/// Cloud service authentication flow configuration
pub struct CloudServiceAuthFlow {
    /// Provider identifier
    provider_id: ProviderId,

    /// Provider-specific parameters to collect
    required_params: Vec<UrlParameter>,
}

impl CloudServiceAuthFlow {
    /// Creates a new cloud service auth flow
    pub fn new(provider_id: ProviderId, required_params: Vec<UrlParameter>) -> Self {
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

        Self::new(provider_id, params)
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

        Self::new(provider_id, params)
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
impl AuthenticationFlow for CloudServiceAuthFlow {
    fn auth_method_type(&self) -> AuthMethodType {
        AuthMethodType::ApiKey
    }

    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        Ok(AuthInitiation::ApiKeyPrompt { required_params: self.required_params.clone() })
    }

    async fn poll_until_complete(
        &self,
        _context: &AuthContext,
        _timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError> {
        Err(AuthFlowError::PollFailed(
            "Cloud service authentication requires manual API key and parameter input".to_string(),
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

                // Validate all required parameters
                self.validate_all_parameters(&url_params)?;

                // Create credential with API key and URL parameters
                let mut credential =
                    ProviderCredential::new_api_key(self.provider_id.clone(), api_key);
                credential.url_params = url_params;

                Ok(credential)
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "CloudServiceAuthFlow requires AuthResult::ApiKey".to_string(),
            )),
        }
    }

    async fn refresh(
        &self,
        _credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        Err(AuthFlowError::RefreshFailed(
            "Cloud service credentials do not support automatic refresh".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn vertex_ai_fixture() -> CloudServiceAuthFlow {
        CloudServiceAuthFlow::vertex_ai(ProviderId::VertexAi)
    }

    fn azure_fixture() -> CloudServiceAuthFlow {
        CloudServiceAuthFlow::azure_openai(ProviderId::Azure)
    }

    #[test]
    fn test_auth_method_type() {
        let flow = vertex_ai_fixture();
        assert_eq!(flow.auth_method_type(), AuthMethodType::ApiKey);
    }

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
    async fn test_poll_returns_error() {
        let flow = vertex_ai_fixture();
        let context = AuthContext::default();
        let result = flow
            .poll_until_complete(&context, Duration::from_secs(60))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("manual"));
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
    async fn test_complete_with_empty_api_key() {
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
    async fn test_complete_with_wrong_result_type() {
        let flow = vertex_ai_fixture();

        let auth_result = AuthResult::OAuthTokens {
            access_token: "token".to_string(),
            refresh_token: None,
            expires_in: None,
        };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires AuthResult::ApiKey"));
    }

    #[tokio::test]
    async fn test_refresh_returns_error() {
        let flow = vertex_ai_fixture();
        let credential =
            ProviderCredential::new_api_key(ProviderId::VertexAi, "test-key".to_string());

        let result = flow.refresh(&credential).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("do not support"));
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
