use derive_setters::Setters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use url::Url;

use crate::Model;

/// --- IMPORTANT ---
/// The order of providers is important because that would be order in which the
/// providers will be resolved
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, PartialOrd, Ord, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Forge,
    #[serde(rename = "openai")]
    OpenAI,
    OpenRouter,
    Requesty,
    Zai,
    ZaiCoding,
    Cerebras,
    Xai,
    Anthropic,
    VertexAi,
    BigModel,
    Azure,
    GithubCopilot,
    #[serde(rename = "openai_compatible")]
    OpenAICompatible,
    AnthropicCompatible,

    // Dynamic custom providers
    #[serde(untagged)]
    Custom(String),
}

impl ProviderId {
    /// Check if this is a custom provider
    pub fn is_custom(&self) -> bool {
        matches!(self, ProviderId::Custom(_))
    }

    /// Get the string representation for both built-in and custom providers
    pub fn as_str(&self) -> &str {
        match self {
            ProviderId::Forge => "forge",
            ProviderId::OpenAI => "openai",
            ProviderId::OpenRouter => "open_router",
            ProviderId::Requesty => "requesty",
            ProviderId::Zai => "zai",
            ProviderId::ZaiCoding => "zai_coding",
            ProviderId::Cerebras => "cerebras",
            ProviderId::Xai => "xai",
            ProviderId::Anthropic => "anthropic",
            ProviderId::VertexAi => "vertex_ai",
            ProviderId::BigModel => "big_model",
            ProviderId::Azure => "azure",
            ProviderId::GithubCopilot => "github_copilot",
            ProviderId::OpenAICompatible => "openai_compatible",
            ProviderId::AnthropicCompatible => "anthropic_compatible",
            ProviderId::Custom(name) => name,
        }
    }

    /// Helper for built-in provider parsing
    fn from_str_builtin(s: &str) -> Result<Self, String> {
        match s {
            "forge" => Ok(ProviderId::Forge),
            "openai" => Ok(ProviderId::OpenAI),
            "open_router" => Ok(ProviderId::OpenRouter),
            "requesty" => Ok(ProviderId::Requesty),
            "zai" => Ok(ProviderId::Zai),
            "zai_coding" => Ok(ProviderId::ZaiCoding),
            "cerebras" => Ok(ProviderId::Cerebras),
            "xai" => Ok(ProviderId::Xai),
            "anthropic" => Ok(ProviderId::Anthropic),
            "vertex_ai" => Ok(ProviderId::VertexAi),
            "big_model" => Ok(ProviderId::BigModel),
            "azure" => Ok(ProviderId::Azure),
            "github_copilot" => Ok(ProviderId::GithubCopilot),
            "openai_compatible" => Ok(ProviderId::OpenAICompatible),
            "anthropic_compatible" => Ok(ProviderId::AnthropicCompatible),
            _ => Err(format!("Unknown built-in provider: {}", s)),
        }
    }
}

impl From<&str> for ProviderId {
    fn from(s: &str) -> Self {
        s.parse()
            .unwrap_or_else(|_| ProviderId::Custom(s.to_string()))
    }
}

impl std::str::FromStr for ProviderId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // First try built-in providers
        if let Ok(built_in) = Self::from_str_builtin(s) {
            return Ok(built_in);
        }

        // Fallback to custom provider
        Ok(ProviderId::Custom(s.to_string()))
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderResponse {
    OpenAI,
    Anthropic,
}

/// Represents the source of models for a provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Models {
    /// Models are fetched from a URL
    Url(Url),
    /// Models are hardcoded in the configuration
    Hardcoded(Vec<Model>),
}

/// Providers that can be used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
pub struct Provider {
    pub id: ProviderId,
    pub response: ProviderResponse,
    pub url: Url,
    pub key: Option<String>,
    pub models: Models,
}

#[cfg(test)]
mod test_helpers {
    use super::*;
    /// Test helper for creating a ZAI provider
    pub(super) fn zai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Zai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            key: Some(key.into()),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub(super) fn zai_coding(key: &str) -> Provider {
        Provider {
            id: ProviderId::ZaiCoding,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            key: Some(key.into()),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating an OpenAI provider
    pub(super) fn openai(key: &str) -> Provider {
        Provider {
            id: ProviderId::OpenAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            key: Some(key.into()),
            models: Models::Url(Url::parse("https://api.openai.com/v1/models").unwrap()),
        }
    }

    /// Test helper for creating an XAI provider
    pub(super) fn xai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            key: Some(key.into()),
            models: Models::Url(Url::parse("https://api.x.ai/v1/models").unwrap()),
        }
    }

    /// Test helper for creating a Vertex AI provider
    pub(super) fn vertex_ai(key: &str, project_id: &str, location: &str) -> Provider {
        let (chat_url, model_url) = if location == "global" {
            (
                format!(
                    "https://aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/chat/completions",
                    project_id, location
                ),
                format!(
                    "https://aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/models",
                    project_id, location
                ),
            )
        } else {
            (
                format!(
                    "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/chat/completions",
                    location, project_id, location
                ),
                format!(
                    "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/endpoints/openapi/models",
                    location, project_id, location
                ),
            )
        };
        Provider {
            id: ProviderId::VertexAi,
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            key: Some(key.into()),
            models: Models::Url(Url::parse(&model_url).unwrap()),
        }
    }

    /// Test helper for creating an Azure provider
    pub(super) fn azure(
        key: &str,
        resource_name: &str,
        deployment_name: &str,
        api_version: &str,
    ) -> Provider {
        let chat_url = format!(
            "https://{}.openai.azure.com/openai/deployments/{}/chat/completions?api-version={}",
            resource_name, deployment_name, api_version
        );
        let model_url = format!(
            "https://{}.openai.azure.com/openai/models?api-version={}",
            resource_name, api_version
        );

        Provider {
            id: ProviderId::Azure,
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            key: Some(key.into()),
            models: Models::Url(Url::parse(&model_url).unwrap()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;

    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_xai() {
        let fixture = "test_key";
        let actual = xai(fixture);
        let expected = Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::from_str("https://api.x.ai/v1/chat/completions").unwrap(),
            key: Some(fixture.to_string()),
            models: Models::Url(Url::from_str("https://api.x.ai/v1/models").unwrap()),
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_xai_with_direct_comparison() {
        let fixture_xai = xai("key");
        assert_eq!(fixture_xai.id, ProviderId::Xai);

        let fixture_other = openai("key");
        assert_ne!(fixture_other.id, ProviderId::Xai);
    }

    #[test]
    fn test_zai_coding_to_chat_url() {
        let fixture = zai_coding("test_key");
        let actual = fixture.url.clone();
        let expected = Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_zai_coding_to_model_url() {
        let fixture = zai_coding("test_key");
        let actual = fixture.models.clone();
        let expected = Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_chat_url() {
        let fixture = zai("test_key");
        let actual = fixture.url.clone();
        let expected = Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_regular_zai_to_model_url() {
        let fixture = zai("test_key");
        let actual = fixture.models.clone();
        let expected = Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_vertex_ai_global_location() {
        let fixture = vertex_ai("test_token", "forge-452914", "global");
        let actual = fixture.url.clone();
        let expected = Url::parse("https://aiplatform.googleapis.com/v1/projects/forge-452914/locations/global/endpoints/openapi/chat/completions").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_vertex_ai_regular_location() {
        let fixture = vertex_ai("test_token", "test_project", "us-central1");
        let actual = fixture.url.clone();
        let expected = Url::parse("https://us-central1-aiplatform.googleapis.com/v1/projects/test_project/locations/us-central1/endpoints/openapi/chat/completions").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_azure_provider() {
        let fixture = azure("test_key", "my-resource", "gpt-4", "2024-02-15-preview");

        // Check chat completion URL (url field now contains the chat completion URL)
        let actual_chat = fixture.url.clone();
        let expected_chat = Url::parse("https://my-resource.openai.azure.com/openai/deployments/gpt-4/chat/completions?api-version=2024-02-15-preview").unwrap();
        assert_eq!(actual_chat, expected_chat);

        // Check model URL
        let actual_model = fixture.models.clone();
        let expected_model = Models::Url(
            Url::parse(
                "https://my-resource.openai.azure.com/openai/models?api-version=2024-02-15-preview",
            )
            .unwrap(),
        );
        assert_eq!(actual_model, expected_model);

        assert_eq!(fixture.id, ProviderId::Azure);
        assert_eq!(fixture.response, ProviderResponse::OpenAI);
    }

    #[test]
    fn test_azure_provider_with_different_params() {
        let fixture = azure("another_key", "east-us", "gpt-35-turbo", "2023-05-15");

        // Check chat completion URL
        let actual_chat = fixture.url.clone();
        let expected_chat = Url::parse("https://east-us.openai.azure.com/openai/deployments/gpt-35-turbo/chat/completions?api-version=2023-05-15").unwrap();
        assert_eq!(actual_chat, expected_chat);

        // Check model URL
        let actual_model = fixture.models.clone();
        let expected_model = Models::Url(
            Url::parse("https://east-us.openai.azure.com/openai/models?api-version=2023-05-15")
                .unwrap(),
        );
        assert_eq!(actual_model, expected_model);
    }

    #[test]
    fn test_custom_provider_id() {
        let custom_id = ProviderId::Custom("vllmlocal".to_string());
        assert!(custom_id.is_custom());
        assert_eq!(custom_id.as_str(), "vllmlocal");
        assert_eq!(custom_id.to_string(), "vllmlocal");
    }

    #[test]
    fn test_builtin_provider_from_str() {
        let openai_id = ProviderId::from_str("openai").unwrap();
        assert!(!openai_id.is_custom());
        assert_eq!(openai_id, ProviderId::OpenAI);
        assert_eq!(openai_id.as_str(), "openai");

        let anthropic_id = ProviderId::from_str("anthropic").unwrap();
        assert!(!anthropic_id.is_custom());
        assert_eq!(anthropic_id, ProviderId::Anthropic);
        assert_eq!(anthropic_id.as_str(), "anthropic");
    }

    #[test]
    fn test_custom_provider_from_str() {
        let custom_id = ProviderId::from_str("my_custom_provider").unwrap();
        assert!(custom_id.is_custom());
        assert_eq!(
            custom_id,
            ProviderId::Custom("my_custom_provider".to_string())
        );
        assert_eq!(custom_id.as_str(), "my_custom_provider");
    }

    #[test]
    fn test_provider_id_from_trait() {
        // Built-in provider
        let openai_id = ProviderId::from("openai");
        assert_eq!(openai_id, ProviderId::OpenAI);
        assert!(!openai_id.is_custom());

        // Custom provider
        let custom_id = ProviderId::from("my_provider");
        assert!(custom_id.is_custom());
        assert_eq!(custom_id, ProviderId::Custom("my_provider".to_string()));
    }

    #[test]
    fn test_backward_compatibility() {
        // Verify all built-in providers work as before
        let built_in_providers = [
            ("forge", ProviderId::Forge),
            ("openai", ProviderId::OpenAI),
            ("open_router", ProviderId::OpenRouter),
            ("requesty", ProviderId::Requesty),
            ("zai", ProviderId::Zai),
            ("zai_coding", ProviderId::ZaiCoding),
            ("cerebras", ProviderId::Cerebras),
            ("xai", ProviderId::Xai),
            ("anthropic", ProviderId::Anthropic),
            ("vertex_ai", ProviderId::VertexAi),
            ("big_model", ProviderId::BigModel),
            ("azure", ProviderId::Azure),
        ];

        for (name, expected_id) in built_in_providers {
            let parsed_id = ProviderId::from_str(name).unwrap();
            assert_eq!(parsed_id, expected_id, "Failed to parse {}", name);
            assert!(!parsed_id.is_custom(), "{} should not be custom", name);
            assert_eq!(
                parsed_id.as_str(),
                name,
                "String representation mismatch for {}",
                name
            );
        }
    }

    #[test]
    fn test_custom_provider_serialization() {
        let custom_id = ProviderId::Custom("test_provider".to_string());

        // Test Display trait
        assert_eq!(custom_id.to_string(), "test_provider");

        // Test that we can parse it back
        let parsed: ProviderId = custom_id.to_string().parse().unwrap();
        assert_eq!(parsed, custom_id);
    }

    #[test]
    fn test_builtin_provider_serialization() {
        let openai_id = ProviderId::OpenAI;

        // Test Display trait
        assert_eq!(openai_id.to_string(), "openai");

        // Test that we can parse it back
        let parsed: ProviderId = openai_id.to_string().parse().unwrap();
        assert_eq!(parsed, openai_id);
    }
}
