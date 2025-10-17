/// Provider metadata service - centralized provider configuration
///
/// This module provides a centralized service for accessing provider-specific
/// configuration including authentication methods, OAuth settings, environment
/// variable names, and display names.
use forge_app::dto::ProviderId;

use super::{AuthMethod, AuthMethodType, OAuthConfig};

/// Provider metadata containing configuration for authentication and display
pub struct ProviderMetadata {
    pub provider_id: ProviderId,
    pub auth_methods: Vec<AuthMethod>,
    pub env_var_names: Vec<String>,
    pub display_name: String,
}

/// Service providing provider-specific metadata and configuration
///
/// This service centralizes all provider-specific configuration that was
/// previously scattered across the codebase, particularly in UI layers. It
/// provides methods to retrieve authentication methods, environment variable
/// names, and display information for all providers.
pub struct ProviderMetadataService;

impl ProviderMetadataService {
    /// Get all authentication methods available for a provider
    ///
    /// Returns a list of configured authentication methods for the specified
    /// provider. Providers can support multiple authentication methods
    /// (e.g., API key and OAuth).
    ///
    /// # Arguments
    /// * `provider_id` - The provider to get authentication methods for
    pub fn get_auth_methods(provider_id: &ProviderId) -> Vec<AuthMethod> {
        match provider_id {
            ProviderId::GithubCopilot => vec![AuthMethod::oauth_device(
                "GitHub OAuth",
                Some("Use your GitHub account to access Copilot".to_string()),
                OAuthConfig::device_flow(
                    "https://github.com/login/device/code",
                    "https://github.com/login/oauth/access_token",
                    "Iv1.b507a08c87ecfe98",
                    vec!["read:user".to_string()],
                )
                .with_token_refresh_url("https://api.github.com/copilot_internal/v2/token")
                .with_custom_header("User-Agent", "GitHubCopilotChat/0.26.7"),
            )],
            ProviderId::Forge => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::OpenAI => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::Anthropic => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::OpenRouter => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::Requesty => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::Zai => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::ZaiCoding => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::Cerebras => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::Xai => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::VertexAi => vec![AuthMethod::api_key("Auth Token", None)],
            ProviderId::BigModel => vec![AuthMethod::api_key("API Key", None)],
            ProviderId::Azure => vec![AuthMethod::api_key("API Key", None)],
        }
    }

    /// Get environment variable names that may contain credentials for a
    /// provider
    ///
    /// Returns a list of environment variable names in priority order. The
    /// first variable that exists will be used when importing credentials
    /// from the environment.
    ///
    /// # Arguments
    /// * `provider_id` - The provider to get environment variables for
    pub fn get_env_var_names(provider_id: &ProviderId) -> Vec<String> {
        match provider_id {
            ProviderId::Forge => vec!["FORGE_API_KEY".to_string()],
            ProviderId::GithubCopilot => vec![
                "GITHUB_COPILOT_API_KEY".to_string(),
                "GITHUB_TOKEN".to_string(),
            ],
            ProviderId::OpenAI => vec!["OPENAI_API_KEY".to_string()],
            ProviderId::Anthropic => vec!["ANTHROPIC_API_KEY".to_string()],
            ProviderId::OpenRouter => vec!["OPENROUTER_API_KEY".to_string()],
            ProviderId::Requesty => vec!["REQUESTY_API_KEY".to_string()],
            ProviderId::Zai => vec!["ZAI_API_KEY".to_string()],
            ProviderId::ZaiCoding => vec!["ZAI_CODING_API_KEY".to_string()],
            ProviderId::Cerebras => vec!["CEREBRAS_API_KEY".to_string()],
            ProviderId::Xai => vec!["XAI_API_KEY".to_string()],
            ProviderId::VertexAi => vec!["VERTEX_AI_AUTH_TOKEN".to_string()],
            ProviderId::BigModel => vec!["BIG_MODEL_API_KEY".to_string()],
            ProviderId::Azure => vec!["AZURE_API_KEY".to_string()],
        }
    }

    /// Get the primary OAuth authentication method for a provider
    ///
    /// Returns the OAuth method if the provider supports OAuth authentication,
    /// or None if the provider only supports API key authentication.
    ///
    /// # Arguments
    /// * `provider_id` - The provider to get OAuth method for
    pub fn get_oauth_method(provider_id: &ProviderId) -> Option<AuthMethod> {
        Self::get_auth_methods(provider_id).into_iter().find(|m| {
            matches!(
                m.method_type,
                AuthMethodType::OAuthDevice | AuthMethodType::OAuthCode
            )
        })
    }

    /// Get the human-readable display name for a provider
    ///
    /// Returns the formatted display name suitable for UI presentation.
    ///
    /// # Arguments
    /// * `provider_id` - The provider to get display name for
    pub fn get_display_name(provider_id: &ProviderId) -> String {
        match provider_id {
            ProviderId::Forge => "Forge".to_string(),
            ProviderId::GithubCopilot => "GitHub Copilot".to_string(),
            ProviderId::OpenAI => "OpenAI".to_string(),
            ProviderId::Anthropic => "Anthropic".to_string(),
            ProviderId::OpenRouter => "OpenRouter".to_string(),
            ProviderId::Requesty => "Requesty".to_string(),
            ProviderId::Zai => "ZAI".to_string(),
            ProviderId::ZaiCoding => "ZAI Coding".to_string(),
            ProviderId::Cerebras => "Cerebras".to_string(),
            ProviderId::Xai => "xAI".to_string(),
            ProviderId::VertexAi => "Google Vertex AI".to_string(),
            ProviderId::BigModel => "BigModel".to_string(),
            ProviderId::Azure => "Azure OpenAI".to_string(),
        }
    }

    /// Get complete metadata for a provider
    ///
    /// Returns all metadata for a provider including authentication methods,
    /// environment variables, and display information.
    ///
    /// # Arguments
    /// * `provider_id` - The provider to get metadata for
    pub fn get_metadata(provider_id: &ProviderId) -> ProviderMetadata {
        ProviderMetadata {
            provider_id: *provider_id,
            auth_methods: Self::get_auth_methods(provider_id),
            env_var_names: Self::get_env_var_names(provider_id),
            display_name: Self::get_display_name(provider_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_github_copilot_oauth_method() {
        let method = ProviderMetadataService::get_oauth_method(&ProviderId::GithubCopilot);

        assert!(method.is_some());
        let method = method.unwrap();
        assert_eq!(method.method_type, AuthMethodType::OAuthDevice);
        assert_eq!(method.label, "GitHub OAuth");

        let config = method.oauth_config.unwrap();
        assert_eq!(
            config.device_code_url,
            Some("https://github.com/login/device/code".to_string())
        );
        assert_eq!(
            config.device_token_url,
            Some("https://github.com/login/oauth/access_token".to_string())
        );
        assert_eq!(config.client_id, "Iv1.b507a08c87ecfe98");
        assert_eq!(config.scopes, vec!["read:user"]);
        assert_eq!(
            config.token_refresh_url,
            Some("https://api.github.com/copilot_internal/v2/token".to_string())
        );
    }

    #[test]
    fn test_openai_api_key_only() {
        let method = ProviderMetadataService::get_oauth_method(&ProviderId::OpenAI);
        assert!(method.is_none());

        let methods = ProviderMetadataService::get_auth_methods(&ProviderId::OpenAI);
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].method_type, AuthMethodType::ApiKey);
    }

    #[test]
    fn test_env_var_names() {
        let vars = ProviderMetadataService::get_env_var_names(&ProviderId::OpenAI);
        assert_eq!(vars, vec!["OPENAI_API_KEY"]);

        let vars = ProviderMetadataService::get_env_var_names(&ProviderId::GithubCopilot);
        assert_eq!(vars, vec!["GITHUB_COPILOT_API_KEY", "GITHUB_TOKEN"]);

        let vars = ProviderMetadataService::get_env_var_names(&ProviderId::Anthropic);
        assert_eq!(vars, vec!["ANTHROPIC_API_KEY"]);
    }

    #[test]
    fn test_display_names() {
        assert_eq!(
            ProviderMetadataService::get_display_name(&ProviderId::GithubCopilot),
            "GitHub Copilot"
        );
        assert_eq!(
            ProviderMetadataService::get_display_name(&ProviderId::OpenAI),
            "OpenAI"
        );
        assert_eq!(
            ProviderMetadataService::get_display_name(&ProviderId::VertexAi),
            "Google Vertex AI"
        );
    }

    #[test]
    fn test_complete_metadata() {
        let metadata = ProviderMetadataService::get_metadata(&ProviderId::GithubCopilot);

        assert_eq!(metadata.provider_id, ProviderId::GithubCopilot);
        assert_eq!(metadata.display_name, "GitHub Copilot");
        assert_eq!(metadata.auth_methods.len(), 1);
        assert_eq!(
            metadata.env_var_names,
            vec!["GITHUB_COPILOT_API_KEY", "GITHUB_TOKEN"]
        );
    }

    #[test]
    fn test_all_providers_have_auth_methods() {
        use strum::IntoEnumIterator;

        for provider_id in ProviderId::iter() {
            let methods = ProviderMetadataService::get_auth_methods(&provider_id);
            assert!(
                !methods.is_empty(),
                "Provider {} has no auth methods defined",
                provider_id
            );
        }
    }

    #[test]
    fn test_all_providers_have_env_vars() {
        use strum::IntoEnumIterator;

        for provider_id in ProviderId::iter() {
            let env_vars = ProviderMetadataService::get_env_var_names(&provider_id);
            assert!(
                !env_vars.is_empty(),
                "Provider {} has no environment variables defined",
                provider_id
            );
        }
    }

    #[test]
    fn test_all_providers_have_display_names() {
        use strum::IntoEnumIterator;

        for provider_id in ProviderId::iter() {
            let display_name = ProviderMetadataService::get_display_name(&provider_id);
            assert!(
                !display_name.is_empty(),
                "Provider {} has no display name",
                provider_id
            );
        }
    }
}
