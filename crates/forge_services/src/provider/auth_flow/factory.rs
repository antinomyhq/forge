/// Authentication flow factory for creating provider-specific flows
///
/// This module provides a factory for instantiating the correct
/// `AuthenticationFlow` implementation based on provider metadata and
/// authentication method configuration.
use std::sync::Arc;

use forge_app::dto::{ProviderId, ProviderResponse, UrlParameter};

use super::{
    ApiKeyAuthFlow, AuthenticationFlow, CloudServiceAuthFlow, CustomProviderAuthFlow,
    OAuthCodeFlow, OAuthDeviceFlow, OAuthWithApiKeyFlow,
};
use crate::provider::{AuthMethod, AuthMethodType, ForgeOAuthService, GitHubCopilotService};

/// Infrastructure requirements for creating authentication flows
///
/// This trait defines the minimal set of services needed to instantiate
/// authentication flows. Implementations should provide access to OAuth
/// services, HTTP clients, and provider-specific services.
pub trait AuthFlowInfra: Send + Sync {
    /// Returns the OAuth service for token operations
    fn oauth_service(&self) -> Arc<ForgeOAuthService>;

    /// Returns the GitHub Copilot service for API key exchange
    fn github_copilot_service(&self) -> Arc<GitHubCopilotService>;
}

/// Factory for creating authentication flow implementations
///
/// The factory examines provider metadata and authentication method
/// configuration to instantiate the appropriate `AuthenticationFlow`
/// implementation. It handles dependency injection for OAuth services, HTTP
/// clients, and provider-specific services.
pub struct AuthFlowFactory;

impl AuthFlowFactory {
    /// Creates an authentication flow for the specified provider and method
    ///
    /// # Arguments
    /// * `provider_id` - The provider to create a flow for
    /// * `auth_method` - The authentication method configuration
    /// * `infra` - Infrastructure services (OAuth, HTTP, etc.)
    ///
    /// # Returns
    /// A boxed trait object implementing `AuthenticationFlow`
    ///
    /// # Errors
    /// Returns error if the authentication method type is unsupported or
    /// required configuration is missing
    pub fn create_flow<I>(
        provider_id: &ProviderId,
        auth_method: &AuthMethod,
        infra: Arc<I>,
    ) -> anyhow::Result<Box<dyn AuthenticationFlow>>
    where
        I: AuthFlowInfra + 'static,
    {
        match auth_method.method_type {
            AuthMethodType::ApiKey => {
                // Check if this is a cloud provider that needs URL parameters
                let required_params = Self::get_provider_params(provider_id);

                if required_params.is_empty() {
                    // Simple API key authentication
                    Ok(Box::new(ApiKeyAuthFlow::new(
                        provider_id.clone(),
                        auth_method.label.clone(),
                        auth_method.description.clone(),
                    )))
                } else {
                    // Cloud service with URL parameters
                    let flow = CloudServiceAuthFlow::new(
                        provider_id.clone(),
                        required_params,
                        auth_method.label.clone(),
                    );
                    let flow = if let Some(desc) = &auth_method.description {
                        flow.with_description(desc)
                    } else {
                        flow
                    };
                    Ok(Box::new(flow))
                }
            }

            AuthMethodType::OAuthDevice => {
                let config = auth_method
                    .oauth_config
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OAuth device flow requires oauth_config"))?;

                // Check if this is GitHub Copilot (OAuth with API key exchange)
                if config.token_refresh_url.is_some() {
                    let github_service = infra.github_copilot_service();
                    Ok(Box::new(OAuthWithApiKeyFlow::new(
                        provider_id.clone(),
                        config.clone(),
                        infra.oauth_service(),
                        github_service,
                    )))
                } else {
                    Ok(Box::new(OAuthDeviceFlow::new(
                        provider_id.clone(),
                        config.clone(),
                        infra.oauth_service(),
                    )))
                }
            }

            AuthMethodType::OAuthCode => {
                let config = auth_method
                    .oauth_config
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OAuth code flow requires oauth_config"))?;

                Ok(Box::new(OAuthCodeFlow::new(
                    provider_id.clone(),
                    config.clone(),
                    infra.oauth_service(),
                )))
            }
        }
    }

    /// Creates a custom provider authentication flow
    ///
    /// Custom providers use a separate flow that prompts for provider-specific
    /// configuration (base URL, model ID, compatibility mode).
    ///
    /// # Arguments
    /// * `compatibility_mode` - Whether the provider is OpenAI or Anthropic
    ///   compatible
    ///
    /// # Returns
    /// A boxed custom provider authentication flow
    pub fn create_custom_provider_flow(
        compatibility_mode: ProviderResponse,
    ) -> Box<dyn AuthenticationFlow> {
        Box::new(CustomProviderAuthFlow::new(compatibility_mode))
    }

    /// Gets required URL parameters for cloud providers
    ///
    /// Returns parameter definitions for providers that require additional
    /// configuration beyond API keys (e.g., Vertex AI project_id, Azure
    /// resource_name).
    fn get_provider_params(provider_id: &ProviderId) -> Vec<UrlParameter> {
        match provider_id {
            ProviderId::VertexAi => Self::vertex_ai_params(),
            ProviderId::Azure => Self::azure_params(),
            _ => vec![],
        }
    }

    /// Returns Vertex AI required parameters
    fn vertex_ai_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::new("project_id", "GCP Project ID")
                .with_description("Your Google Cloud project ID")
                .with_required(true)
                .with_validation_pattern(r"^[a-z][a-z0-9-]{4,28}[a-z0-9]$"),
            UrlParameter::new("location", "Location")
                .with_description("GCP region (e.g., us-central1) or 'global'")
                .with_default_value("us-central1")
                .with_required(true),
        ]
    }

    /// Returns Azure OpenAI required parameters
    fn azure_params() -> Vec<UrlParameter> {
        vec![
            UrlParameter::new("resource_name", "Azure Resource Name")
                .with_description("Your Azure OpenAI resource name")
                .with_required(true),
            UrlParameter::new("deployment_name", "Deployment Name")
                .with_description("Your model deployment name")
                .with_required(true),
            UrlParameter::new("api_version", "API Version")
                .with_description("Azure API version")
                .with_default_value("2024-02-15-preview")
                .with_required(true),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{AuthMethod, OAuthConfig};

    /// Mock infrastructure for testing
    struct MockInfra {
        oauth_service: Arc<ForgeOAuthService>,
        github_service: Arc<GitHubCopilotService>,
    }

    impl MockInfra {
        fn new() -> Self {
            Self {
                oauth_service: Arc::new(ForgeOAuthService),
                github_service: Arc::new(GitHubCopilotService::new()),
            }
        }
    }

    impl AuthFlowInfra for MockInfra {
        fn oauth_service(&self) -> Arc<ForgeOAuthService> {
            self.oauth_service.clone()
        }

        fn github_copilot_service(&self) -> Arc<GitHubCopilotService> {
            self.github_service.clone()
        }
    }

    #[test]
    fn test_create_api_key_flow() {
        let provider_id = ProviderId::OpenAI;
        let auth_method = AuthMethod::api_key("API Key", None);
        let infra = Arc::new(MockInfra::new());

        let result = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra);

        assert!(result.is_ok());
        let flow = result.unwrap();
        assert_eq!(
            flow.auth_method_type(),
            forge_app::dto::AuthMethodType::ApiKey
        );
    }

    #[test]
    fn test_create_api_key_flow_with_vertex_params() {
        let provider_id = ProviderId::VertexAi;
        let auth_method = AuthMethod::api_key("Auth Token", None);
        let infra = Arc::new(MockInfra::new());

        let _flow = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra).unwrap();

        // Vertex AI should get cloud service flow with required params
        // This is validated by the flow's initiate() method returning params
    }

    #[test]
    fn test_create_api_key_flow_with_azure_params() {
        let provider_id = ProviderId::Azure;
        let auth_method = AuthMethod::api_key("API Key", None);
        let infra = Arc::new(MockInfra::new());

        let _flow = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra).unwrap();

        // Azure should get cloud service flow with required params
    }

    #[test]
    fn test_create_oauth_device_flow() {
        let provider_id = ProviderId::Custom("test-oauth".to_string());
        let config = OAuthConfig::device_flow(
            "https://provider.com/device",
            "https://provider.com/token",
            "client-id-123",
            vec!["scope1".to_string()],
        );
        let auth_method = AuthMethod::oauth_device("OAuth Device", None, config);
        let infra = Arc::new(MockInfra::new());

        let result = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra);
        assert!(result.is_ok());
        let flow = result.unwrap();
        assert_eq!(
            flow.auth_method_type(),
            forge_app::dto::AuthMethodType::OAuthDevice
        );
    }

    #[test]
    fn test_create_oauth_with_apikey_flow() {
        let provider_id = ProviderId::GithubCopilot;
        let config = OAuthConfig::device_flow(
            "https://github.com/login/device/code",
            "https://github.com/login/oauth/access_token",
            "client-id",
            vec!["read:user".to_string()],
        )
        .with_token_refresh_url("https://api.github.com/copilot_internal/v2/token");

        let auth_method = AuthMethod::oauth_device("GitHub OAuth", None, config);
        let infra = Arc::new(MockInfra::new());

        let result = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra);
        assert!(result.is_ok());
        let flow = result.unwrap();

        // Should create OAuthWithApiKeyFlow due to token_refresh_url
        assert_eq!(
            flow.auth_method_type(),
            forge_app::dto::AuthMethodType::OAuthDevice
        );
    }

    #[test]
    fn test_create_oauth_code_flow() {
        let provider_id = ProviderId::Custom("test-code".to_string());
        let config = OAuthConfig::code_flow(
            "https://provider.com/authorize",
            "https://provider.com/token",
            "client-id-456",
            vec!["scope1".to_string()],
            "https://provider.com/callback",
            true, // use_pkce
        );

        let auth_method = AuthMethod::oauth_code("OAuth Code", None, config);
        let infra = Arc::new(MockInfra::new());

        let result = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra);
        assert!(result.is_ok());
        let flow = result.unwrap();

        assert_eq!(
            flow.auth_method_type(),
            forge_app::dto::AuthMethodType::OAuthCode
        );
    }

    #[test]
    fn test_create_custom_provider_flow() {
        let flow = AuthFlowFactory::create_custom_provider_flow(ProviderResponse::OpenAI);

        assert_eq!(
            flow.auth_method_type(),
            forge_app::dto::AuthMethodType::ApiKey
        );
    }

    #[test]
    fn test_oauth_device_without_config_fails() {
        let provider_id = ProviderId::OpenAI;
        let auth_method = AuthMethod {
            method_type: AuthMethodType::OAuthDevice,
            label: "OAuth".to_string(),
            description: None,
            oauth_config: None,
        };
        let infra = Arc::new(MockInfra::new());

        let result = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra);

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        assert!(error_msg.contains("requires oauth_config"));
    }

    #[test]
    fn test_oauth_code_without_config_fails() {
        let provider_id = ProviderId::Anthropic;
        let auth_method = AuthMethod {
            method_type: AuthMethodType::OAuthCode,
            label: "OAuth Code".to_string(),
            description: None,
            oauth_config: None,
        };
        let infra = Arc::new(MockInfra::new());

        let result = AuthFlowFactory::create_flow(&provider_id, &auth_method, infra);

        assert!(result.is_err());
        let error_msg = result.err().unwrap().to_string();
        assert!(error_msg.contains("requires oauth_config"));
    }

    #[test]
    fn test_vertex_ai_params_structure() {
        let params = AuthFlowFactory::vertex_ai_params();

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].key, "project_id");
        assert_eq!(params[0].label, "GCP Project ID");
        assert!(params[0].required);
        assert!(params[0].validation_pattern.is_some());

        assert_eq!(params[1].key, "location");
        assert_eq!(params[1].default_value, Some("us-central1".to_string()));
    }

    #[test]
    fn test_azure_params_structure() {
        let params = AuthFlowFactory::azure_params();

        assert_eq!(params.len(), 3);
        assert_eq!(params[0].key, "resource_name");
        assert_eq!(params[1].key, "deployment_name");
        assert_eq!(params[2].key, "api_version");
        assert_eq!(
            params[2].default_value,
            Some("2024-02-15-preview".to_string())
        );
    }
}
