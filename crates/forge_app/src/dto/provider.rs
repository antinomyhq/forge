use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use url::Url;

use super::AuthType;

/// --- IMPORTANT ---
/// The order of providers is important because that would be order in which the
/// providers will be resolved
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    #[serde(alias = "Forge")]
    Forge,
    #[serde(rename = "github_copilot", alias = "GithubCopilot")]
    GithubCopilot,
    #[serde(rename = "openai", alias = "OpenAI")]
    OpenAI,
    #[serde(alias = "OpenRouter")]
    OpenRouter,
    #[serde(alias = "Requesty")]
    Requesty,
    #[serde(alias = "Zai")]
    Zai,
    #[serde(rename = "zai_coding", alias = "ZaiCoding")]
    ZaiCoding,
    #[serde(alias = "Cerebras")]
    Cerebras,
    #[serde(alias = "Xai")]
    Xai,
    #[serde(alias = "Anthropic")]
    Anthropic,
    #[serde(rename = "vertex_ai", alias = "VertexAi")]
    VertexAi,
    #[serde(rename = "big_model", alias = "BigModel")]
    BigModel,
    #[serde(alias = "Azure")]
    Azure,
    /// Custom user-defined provider
    #[serde(rename = "custom", alias = "Custom")]
    Custom(String),
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderId::Forge => write!(f, "forge"),
            ProviderId::GithubCopilot => write!(f, "github_copilot"),
            ProviderId::OpenAI => write!(f, "openai"),
            ProviderId::OpenRouter => write!(f, "open_router"),
            ProviderId::Requesty => write!(f, "requesty"),
            ProviderId::Zai => write!(f, "zai"),
            ProviderId::ZaiCoding => write!(f, "zai_coding"),
            ProviderId::Cerebras => write!(f, "cerebras"),
            ProviderId::Xai => write!(f, "xai"),
            ProviderId::Anthropic => write!(f, "anthropic"),
            ProviderId::VertexAi => write!(f, "vertex_ai"),
            ProviderId::BigModel => write!(f, "big_model"),
            ProviderId::Azure => write!(f, "azure"),
            ProviderId::Custom(name) => write!(f, "custom_{}", name),
        }
    }
}

impl ProviderId {
    /// Returns true if this is a custom provider
    pub fn is_custom(&self) -> bool {
        matches!(self, ProviderId::Custom(_))
    }

    /// Returns the custom provider name if this is a custom provider
    pub fn custom_name(&self) -> Option<&str> {
        match self {
            ProviderId::Custom(name) => Some(name.as_str()),
            _ => None,
        }
    }

    /// Returns all built-in provider IDs (excludes Custom)
    pub fn built_in_providers() -> Vec<ProviderId> {
        vec![
            ProviderId::Forge,
            ProviderId::GithubCopilot,
            ProviderId::OpenAI,
            ProviderId::OpenRouter,
            ProviderId::Requesty,
            ProviderId::Zai,
            ProviderId::ZaiCoding,
            ProviderId::Cerebras,
            ProviderId::Xai,
            ProviderId::Anthropic,
            ProviderId::VertexAi,
            ProviderId::BigModel,
            ProviderId::Azure,
        ]
    }

    /// Returns human-readable display name (PascalCase)
    /// Used for UI display, not serialization
    pub fn display_name(&self) -> String {
        match self {
            ProviderId::Forge => "Forge".to_string(),
            ProviderId::GithubCopilot => "GitHub Copilot".to_string(),
            ProviderId::OpenAI => "OpenAI".to_string(),
            ProviderId::OpenRouter => "OpenRouter".to_string(),
            ProviderId::Requesty => "Requesty".to_string(),
            ProviderId::Zai => "Zai".to_string(),
            ProviderId::ZaiCoding => "ZaiCoding".to_string(),
            ProviderId::Cerebras => "Cerebras".to_string(),
            ProviderId::Xai => "Xai".to_string(),
            ProviderId::Anthropic => "Anthropic".to_string(),
            ProviderId::VertexAi => "Vertex AI".to_string(),
            ProviderId::BigModel => "BigModel".to_string(),
            ProviderId::Azure => "Azure".to_string(),
            ProviderId::Custom(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderResponse {
    OpenAI,
    Anthropic,
}

/// Providers that can be used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
pub struct Provider {
    pub id: ProviderId,
    pub response: ProviderResponse,
    pub url: Url,
    pub key: Option<String>,
    pub model_url: Url,
    pub auth_type: Option<AuthType>,
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
            model_url: Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            auth_type: None,
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub(super) fn zai_coding(key: &str) -> Provider {
        Provider {
            id: ProviderId::ZaiCoding,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            key: Some(key.into()),
            model_url: Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            auth_type: None,
        }
    }

    /// Test helper for creating an OpenAI provider
    pub(super) fn openai(key: &str) -> Provider {
        Provider {
            id: ProviderId::OpenAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            key: Some(key.into()),
            model_url: Url::parse("https://api.openai.com/v1/models").unwrap(),
            auth_type: None,
        }
    }

    /// Test helper for creating an XAI provider
    pub(super) fn xai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            key: Some(key.into()),
            model_url: Url::parse("https://api.x.ai/v1/models").unwrap(),
            auth_type: None,
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
            model_url: Url::parse(&model_url).unwrap(),
            auth_type: None,
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
            model_url: Url::parse(&model_url).unwrap(),
            auth_type: None,
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
            model_url: Url::from_str("https://api.x.ai/v1/models").unwrap(),
            auth_type: None,
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
        let actual = fixture.model_url.clone();
        let expected = Url::parse("https://api.z.ai/api/paas/v4/models").unwrap();
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
        let actual = fixture.model_url.clone();
        let expected = Url::parse("https://api.z.ai/api/paas/v4/models").unwrap();
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
        let actual_model = fixture.model_url.clone();
        let expected_model = Url::parse(
            "https://my-resource.openai.azure.com/openai/models?api-version=2024-02-15-preview",
        )
        .unwrap();
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
        let actual_model = fixture.model_url.clone();
        let expected_model =
            Url::parse("https://east-us.openai.azure.com/openai/models?api-version=2023-05-15")
                .unwrap();
        assert_eq!(actual_model, expected_model);
    }

    #[test]
    fn test_github_copilot_display_name() {
        let actual = ProviderId::GithubCopilot.to_string();
        let expected = "github_copilot";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_provider_id_display() {
        assert_eq!(ProviderId::OpenAI.to_string(), "openai");
        assert_eq!(ProviderId::GithubCopilot.to_string(), "github_copilot");
        assert_eq!(
            ProviderId::Custom("my-llm".to_string()).to_string(),
            "custom_my-llm"
        );
    }

    #[test]
    fn test_provider_id_is_custom() {
        assert!(!ProviderId::OpenAI.is_custom());
        assert!(!ProviderId::Anthropic.is_custom());
        assert!(ProviderId::Custom("test".to_string()).is_custom());
    }

    #[test]
    fn test_provider_id_custom_name() {
        assert_eq!(ProviderId::OpenAI.custom_name(), None);
        assert_eq!(
            ProviderId::Custom("my-provider".to_string()).custom_name(),
            Some("my-provider")
        );
    }

    #[test]
    fn test_provider_id_built_in_providers() {
        let built_in = ProviderId::built_in_providers();
        assert_eq!(built_in.len(), 13); // All non-Custom variants
        assert!(built_in.contains(&ProviderId::OpenAI));
        assert!(built_in.contains(&ProviderId::Anthropic));
        assert!(!built_in.iter().any(|p| p.is_custom()));
    }

    #[test]
    fn test_provider_id_serialization() {
        let openai = ProviderId::OpenAI;
        let json = serde_json::to_string(&openai).unwrap();
        assert_eq!(json, r#""openai""#);

        let custom = ProviderId::Custom("my-llm".to_string());
        let json = serde_json::to_string(&custom).unwrap();
        assert_eq!(json, r#"{"custom":"my-llm"}"#);
    }

    #[test]
    fn test_provider_id_deserialization() {
        let openai: ProviderId = serde_json::from_str(r#""openai""#).unwrap();
        assert_eq!(openai, ProviderId::OpenAI);

        let custom: ProviderId = serde_json::from_str(r#"{"custom":"my-llm"}"#).unwrap();
        assert_eq!(custom, ProviderId::Custom("my-llm".to_string()));
    }
}
