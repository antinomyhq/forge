//! Custom Provider Authentication Flow
//!
//! Implements authentication for user-defined OpenAI-compatible or
//! Anthropic-compatible providers. This allows users to register custom
//! endpoints like LocalAI, vLLM, Ollama, or corporate proxies.
//!
//! ## Supported Use Cases
//!
//! - **Self-hosted LLM servers** (LocalAI, vLLM, Ollama with OpenAI
//!   compatibility)
//! - **Private cloud deployments** with OpenAI-compatible APIs
//! - **Custom fine-tuned models** on dedicated infrastructure
//! - **Corporate proxies** providing OpenAI/Anthropic-compatible interfaces
//!
//! ## Flow
//!
//! 1. **Initiate**: Returns prompt for provider details (name, base_url,
//!    model_id, api_key)
//! 2. **Poll**: Not applicable (manual input required)
//! 3. **Complete**: Validates inputs and creates credential with custom
//!    configuration
//! 4. **Refresh**: Not supported (custom API keys don't auto-refresh)
//! 5. **Validate**: Returns true (assumes valid, optionally pings health
//!    endpoint)

use std::time::Duration;

use forge_app::dto::{
    AuthContext, AuthInitiation, AuthMethodType, AuthResult, ProviderCredential, ProviderId,
    ProviderResponse, UrlParameter,
};
use url::Url;

use crate::provider::auth_flow::AuthenticationFlow;
use crate::provider::auth_flow::error::AuthFlowError;

/// Custom provider authentication flow
pub struct CustomProviderAuthFlow {
    /// Compatibility mode for this custom provider (OpenAI or Anthropic)
    compatibility_mode: ProviderResponse,
}

impl CustomProviderAuthFlow {
    /// Creates a new custom provider auth flow
    pub fn new(compatibility_mode: ProviderResponse) -> Self {
        Self { compatibility_mode }
    }

    /// Creates an OpenAI-compatible custom provider flow
    pub fn openai_compatible() -> Self {
        Self::new(ProviderResponse::OpenAI)
    }

    /// Creates an Anthropic-compatible custom provider flow
    pub fn anthropic_compatible() -> Self {
        Self::new(ProviderResponse::Anthropic)
    }

    /// Gets the required parameters for custom provider registration
    fn required_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::required("provider_name", "Provider Name")
                .with_description("Display name for this provider (e.g., 'My Local LLM')"),
            UrlParameter::required("base_url", "Base URL")
                .with_description("API endpoint (e.g., http://localhost:8080/v1)")
                .with_validation(r"^https?://.+"),
            UrlParameter::required("model_id", "Model Name").with_description(
                "Model identifier to use in API requests (e.g., 'gpt-4', 'llama-3-70b')",
            ),
            UrlParameter::optional("api_key", "API Key")
                .with_description("Leave empty for local servers without authentication"),
        ]
    }

    /// Validates the base URL is a valid HTTP/HTTPS URL
    fn validate_base_url(base_url: &str) -> Result<(), AuthFlowError> {
        Url::parse(base_url).map_err(|e| {
            AuthFlowError::InvalidBaseUrl(format!("Invalid URL '{}': {}", base_url, e))
        })?;

        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return Err(AuthFlowError::InvalidBaseUrl(
                "Base URL must start with http:// or https://".to_string(),
            ));
        }

        Ok(())
    }

    /// Validates the model ID is non-empty
    fn validate_model_id(model_id: &str) -> Result<(), AuthFlowError> {
        if model_id.trim().is_empty() {
            return Err(AuthFlowError::InvalidParameter(
                "model_id".to_string(),
                "Model ID cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Validates the provider name is non-empty
    fn validate_provider_name(provider_name: &str) -> Result<(), AuthFlowError> {
        if provider_name.trim().is_empty() {
            return Err(AuthFlowError::InvalidParameter(
                "provider_name".to_string(),
                "Provider name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    /// Creates a credential from custom provider details
    fn create_credential(
        &self,
        provider_name: String,
        base_url: String,
        model_id: String,
        api_key: Option<String>,
    ) -> ProviderCredential {
        // Use ProviderId::Custom with the provider name
        let provider_id = ProviderId::Custom(provider_name);

        // Use the new custom provider constructor
        ProviderCredential::new_custom_provider(
            provider_id,
            api_key,
            self.compatibility_mode.clone(),
            base_url,
            model_id,
        )
    }
}

#[async_trait::async_trait]
impl AuthenticationFlow for CustomProviderAuthFlow {
    fn auth_method_type(&self) -> AuthMethodType {
        AuthMethodType::ApiKey
    }

    async fn initiate(&self) -> Result<AuthInitiation, AuthFlowError> {
        Ok(AuthInitiation::CustomProviderPrompt {
            compatibility_mode: self.compatibility_mode.clone(),
            required_params: Self::required_params(),
        })
    }

    async fn poll_until_complete(
        &self,
        _context: &AuthContext,
        _timeout: Duration,
    ) -> Result<AuthResult, AuthFlowError> {
        Err(AuthFlowError::PollFailed(
            "Custom provider registration requires manual input".to_string(),
        ))
    }

    async fn complete(&self, result: AuthResult) -> Result<ProviderCredential, AuthFlowError> {
        match result {
            AuthResult::CustomProvider {
                provider_name,
                base_url,
                model_id,
                api_key,
                compatibility_mode,
            } => {
                // Verify compatibility mode matches
                if compatibility_mode != self.compatibility_mode {
                    return Err(AuthFlowError::CompletionFailed(format!(
                        "Compatibility mode mismatch: expected {:?}, got {:?}",
                        self.compatibility_mode, compatibility_mode
                    )));
                }

                // Validate all inputs
                Self::validate_provider_name(&provider_name)?;
                Self::validate_base_url(&base_url)?;
                Self::validate_model_id(&model_id)?;

                // Create credential with custom provider configuration
                Ok(self.create_credential(provider_name, base_url, model_id, api_key))
            }
            _ => Err(AuthFlowError::CompletionFailed(
                "CustomProviderAuthFlow requires AuthResult::CustomProvider".to_string(),
            )),
        }
    }

    async fn refresh(
        &self,
        _credential: &ProviderCredential,
    ) -> Result<ProviderCredential, AuthFlowError> {
        Err(AuthFlowError::RefreshFailed(
            "Custom provider credentials do not support automatic refresh".to_string(),
        ))
    }

    async fn validate(&self, _credential: &ProviderCredential) -> Result<bool, AuthFlowError> {
        // For custom providers, we assume credentials are valid
        // In a future enhancement, we could ping the health endpoint
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn openai_fixture() -> CustomProviderAuthFlow {
        CustomProviderAuthFlow::openai_compatible()
    }

    fn anthropic_fixture() -> CustomProviderAuthFlow {
        CustomProviderAuthFlow::anthropic_compatible()
    }

    #[test]
    fn test_auth_method_type() {
        let flow = openai_fixture();
        assert_eq!(flow.auth_method_type(), AuthMethodType::ApiKey);
    }

    #[tokio::test]
    async fn test_initiate_openai_compatible() {
        let flow = openai_fixture();
        let result = flow.initiate().await.unwrap();

        match result {
            AuthInitiation::CustomProviderPrompt { compatibility_mode, required_params } => {
                assert_eq!(compatibility_mode, ProviderResponse::OpenAI);
                assert_eq!(required_params.len(), 4);
                assert_eq!(required_params[0].key, "provider_name");
                assert_eq!(required_params[1].key, "base_url");
                assert_eq!(required_params[2].key, "model_id");
                assert_eq!(required_params[3].key, "api_key");
                assert!(!required_params[3].required); // API key is optional
            }
            _ => panic!("Expected CustomProviderPrompt"),
        }
    }

    #[tokio::test]
    async fn test_initiate_anthropic_compatible() {
        let flow = anthropic_fixture();
        let result = flow.initiate().await.unwrap();

        match result {
            AuthInitiation::CustomProviderPrompt { compatibility_mode, .. } => {
                assert_eq!(compatibility_mode, ProviderResponse::Anthropic);
            }
            _ => panic!("Expected CustomProviderPrompt"),
        }
    }

    #[tokio::test]
    async fn test_poll_returns_error() {
        let flow = openai_fixture();
        let context = AuthContext::default();
        let result = flow
            .poll_until_complete(&context, Duration::from_secs(60))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("manual input"));
    }

    #[tokio::test]
    async fn test_complete_with_valid_inputs() {
        let flow = openai_fixture();

        let auth_result = AuthResult::CustomProvider {
            provider_name: "LocalAI GPT-4".to_string(),
            base_url: "http://localhost:8080/v1".to_string(),
            model_id: "gpt-4-local".to_string(),
            api_key: Some("test-key".to_string()),
            compatibility_mode: ProviderResponse::OpenAI,
        };

        let credential = flow.complete(auth_result).await.unwrap();

        // Verify ProviderId is Custom variant
        assert_eq!(
            credential.provider_id,
            ProviderId::Custom("LocalAI GPT-4".to_string())
        );

        // Verify custom provider fields are properly set
        assert_eq!(credential.api_key, Some("test-key".to_string()));
        assert_eq!(
            credential.compatibility_mode,
            Some(ProviderResponse::OpenAI)
        );
        assert_eq!(
            credential.custom_base_url,
            Some("http://localhost:8080/v1".to_string())
        );
        assert_eq!(credential.custom_model_id, Some("gpt-4-local".to_string()));
        assert!(credential.is_custom_provider());
    }

    #[tokio::test]
    async fn test_complete_without_api_key() {
        let flow = openai_fixture();

        let auth_result = AuthResult::CustomProvider {
            provider_name: "Local Ollama".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            model_id: "llama3".to_string(),
            api_key: None, // No API key for local server
            compatibility_mode: ProviderResponse::OpenAI,
        };

        let credential = flow.complete(auth_result).await.unwrap();

        assert_eq!(
            credential.provider_id,
            ProviderId::Custom("Local Ollama".to_string())
        );
        assert_eq!(credential.api_key, None);
        assert_eq!(
            credential.custom_base_url,
            Some("http://localhost:11434/v1".to_string())
        );
        assert_eq!(credential.custom_model_id, Some("llama3".to_string()));
        assert!(credential.is_custom_provider());
    }

    #[tokio::test]
    async fn test_complete_with_invalid_base_url() {
        let flow = openai_fixture();

        let auth_result = AuthResult::CustomProvider {
            provider_name: "Invalid Provider".to_string(),
            base_url: "not-a-valid-url".to_string(),
            model_id: "model".to_string(),
            api_key: None,
            compatibility_mode: ProviderResponse::OpenAI,
        };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid URL"));
    }

    #[tokio::test]
    async fn test_complete_with_empty_provider_name() {
        let flow = openai_fixture();

        let auth_result = AuthResult::CustomProvider {
            provider_name: "   ".to_string(), // Empty/whitespace
            base_url: "http://localhost:8080/v1".to_string(),
            model_id: "model".to_string(),
            api_key: None,
            compatibility_mode: ProviderResponse::OpenAI,
        };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Provider name"));
    }

    #[tokio::test]
    async fn test_complete_with_empty_model_id() {
        let flow = openai_fixture();

        let auth_result = AuthResult::CustomProvider {
            provider_name: "Valid Provider".to_string(),
            base_url: "http://localhost:8080/v1".to_string(),
            model_id: "".to_string(), // Empty
            api_key: None,
            compatibility_mode: ProviderResponse::OpenAI,
        };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Model ID"));
    }

    #[tokio::test]
    async fn test_complete_with_compatibility_mode_mismatch() {
        let flow = openai_fixture(); // Expects OpenAI

        let auth_result = AuthResult::CustomProvider {
            provider_name: "Provider".to_string(),
            base_url: "http://localhost:8080/v1".to_string(),
            model_id: "model".to_string(),
            api_key: None,
            compatibility_mode: ProviderResponse::Anthropic, // But got Anthropic
        };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Compatibility mode mismatch"));
    }

    #[tokio::test]
    async fn test_complete_with_wrong_result_type() {
        let flow = openai_fixture();

        let auth_result =
            AuthResult::ApiKey { api_key: "key".to_string(), url_params: HashMap::new() };

        let result = flow.complete(auth_result).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires AuthResult::CustomProvider"));
    }

    #[tokio::test]
    async fn test_refresh_returns_error() {
        let flow = openai_fixture();
        let credential =
            ProviderCredential::new_api_key(ProviderId::OpenAI, "test-key".to_string());

        let result = flow.refresh(&credential).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("do not support"));
    }

    #[tokio::test]
    async fn test_validate_always_true() {
        let flow = openai_fixture();
        let credential =
            ProviderCredential::new_api_key(ProviderId::OpenAI, "test-key".to_string());

        let is_valid = flow.validate(&credential).await.unwrap();
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_anthropic_complete_with_valid_inputs() {
        let flow = anthropic_fixture();

        let auth_result = AuthResult::CustomProvider {
            provider_name: "Corporate Claude".to_string(),
            base_url: "https://llm.corp.example.com/api".to_string(),
            model_id: "claude-3-opus-internal".to_string(),
            api_key: Some("corp-api-key-12345".to_string()),
            compatibility_mode: ProviderResponse::Anthropic,
        };

        let credential = flow.complete(auth_result).await.unwrap();

        // Verify Anthropic compatibility mode is stored in dedicated field
        assert_eq!(
            credential.compatibility_mode,
            Some(ProviderResponse::Anthropic)
        );
        assert_eq!(
            credential.custom_base_url,
            Some("https://llm.corp.example.com/api".to_string())
        );
        assert_eq!(
            credential.custom_model_id,
            Some("claude-3-opus-internal".to_string())
        );
        assert_eq!(credential.api_key, Some("corp-api-key-12345".to_string()));
    }
}
