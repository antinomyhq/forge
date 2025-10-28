use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, Display, EnumIter, EnumString};
use url::Url;

use super::AuthType;
use super::auth_flow::ApiKey;

/// --- IMPORTANT ---
/// The order of providers is important because that would be order in which the
/// providers will be resolved
#[derive(
    Debug,
    Display,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    EnumIter,
    EnumString,
    AsRefStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ProviderId {
    #[serde(alias = "Forge")]
    #[strum(to_string = "Forge")]
    Forge,
    #[serde(alias = "GithubCopilot")]
    #[strum(to_string = "GithubCopilot")]
    GithubCopilot,
    #[serde(rename = "openai")]
    #[serde(alias = "OpenAI")]
    #[strum(serialize = "openai", to_string = "OpenAI")]
    OpenAI,
    #[serde(rename = "openai_compatible")]
    #[serde(alias = "OpenAICompatible")]
    #[strum(serialize = "openai_compatible", to_string = "OpenAICompatible")]
    OpenAICompatible,
    #[serde(alias = "OpenRouter")]
    #[strum(to_string = "OpenRouter")]
    OpenRouter,
    #[serde(alias = "Requesty")]
    #[strum(to_string = "Requesty")]
    Requesty,
    #[serde(alias = "Zai")]
    #[strum(to_string = "Zai")]
    Zai,
    #[serde(alias = "ZaiCoding")]
    #[strum(to_string = "ZaiCoding")]
    ZaiCoding,
    #[serde(alias = "Cerebras")]
    #[strum(to_string = "Cerebras")]
    Cerebras,
    #[serde(alias = "Xai")]
    #[strum(to_string = "Xai")]
    Xai,
    #[serde(alias = "Anthropic")]
    #[strum(to_string = "Anthropic")]
    Anthropic,
    #[serde(rename = "anthropic_compatible")]
    #[serde(alias = "AnthropicCompatible")]
    #[strum(serialize = "anthropic_compatible", to_string = "AnthropicCompatible")]
    AnthropicCompatible,
    #[serde(alias = "VertexAi")]
    #[strum(to_string = "VertexAi")]
    VertexAi,
    #[serde(alias = "BigModel")]
    #[strum(to_string = "BigModel")]
    BigModel,
    #[serde(alias = "Azure")]
    #[strum(to_string = "Azure")]
    Azure,
}

#[derive(Debug, Display, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ProviderResponse {
    #[serde(rename = "openai")]
    #[strum(serialize = "openai")]
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
    Hardcoded(Vec<crate::domain::Model>),
}

/// Providers that can be used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
pub struct Provider {
    pub id: ProviderId,
    pub response: ProviderResponse,
    pub url: Url,
    pub key: Option<ApiKey>,
    pub models: Models,
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
            key: Some(key.to_string().into()),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
            auth_type: None,
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub(super) fn zai_coding(key: &str) -> Provider {
        Provider {
            id: ProviderId::ZaiCoding,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            key: Some(key.to_string().into()),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
            auth_type: None,
        }
    }

    /// Test helper for creating an OpenAI provider
    pub(super) fn openai(key: &str) -> Provider {
        Provider {
            id: ProviderId::OpenAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            key: Some(key.to_string().into()),
            models: Models::Url(Url::parse("https://api.openai.com/v1/models").unwrap()),
            auth_type: None,
        }
    }

    /// Test helper for creating an XAI provider
    pub(super) fn xai(key: &str) -> Provider {
        Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            key: Some(key.to_string().into()),
            models: Models::Url(Url::parse("https://api.x.ai/v1/models").unwrap()),
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
            key: Some(key.to_string().into()),
            models: Models::Url(Url::parse(&model_url).unwrap()),
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
            key: Some(key.to_string().into()),
            models: Models::Url(Url::parse(&model_url).unwrap()),
            auth_type: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;
    use strum::IntoEnumIterator;

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
            key: Some(fixture.to_string().into()),
            models: Models::Url(Url::from_str("https://api.x.ai/v1/models").unwrap()),
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
    fn test_github_copilot_display_name() {
        let actual = ProviderId::GithubCopilot.to_string();
        let expected = "GithubCopilot";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_provider_id_display() {
        let fixture_openai = ProviderId::OpenAI;
        let actual_openai = fixture_openai.to_string();
        let expected_openai = "OpenAI";
        assert_eq!(actual_openai, expected_openai);

        let fixture_copilot = ProviderId::GithubCopilot;
        let actual_copilot = fixture_copilot.to_string();
        let expected_copilot = "GithubCopilot";
        assert_eq!(actual_copilot, expected_copilot);
    }

    #[test]
    fn test_provider_id_snake_case_identifier() {
        let fixture_openai = ProviderId::OpenAI;
        let actual_openai = serde_json::to_value(fixture_openai)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let expected_openai = "openai".to_string();
        assert_eq!(actual_openai, expected_openai);

        let fixture_copilot = ProviderId::GithubCopilot;
        let actual_copilot = serde_json::to_value(fixture_copilot)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let expected_copilot = "github_copilot".to_string();
        assert_eq!(actual_copilot, expected_copilot);
    }

    #[test]
    fn test_provider_id_built_in_providers() {
        let built_in = ProviderId::iter().collect::<Vec<_>>();
        assert_eq!(built_in.len(), 15); // All provider variants including OpenAICompatible and AnthropicCompatible
        assert!(built_in.contains(&ProviderId::OpenAI));
        assert!(built_in.contains(&ProviderId::Anthropic));
        assert!(built_in.contains(&ProviderId::OpenAICompatible));
        assert!(built_in.contains(&ProviderId::AnthropicCompatible));
    }

    #[test]
    fn test_provider_id_serialization() {
        let openai = ProviderId::OpenAI;
        let json = serde_json::to_string(&openai).unwrap();
        assert_eq!(json, r#""openai""#);

        let anthropic = ProviderId::Anthropic;
        let json = serde_json::to_string(&anthropic).unwrap();
        assert_eq!(json, r#""anthropic""#);
    }

    #[test]
    fn test_provider_id_deserialization() {
        let openai: ProviderId = serde_json::from_str(r#""openai""#).unwrap();
        assert_eq!(openai, ProviderId::OpenAI);

        let anthropic: ProviderId = serde_json::from_str(r#""anthropic""#).unwrap();
        assert_eq!(anthropic, ProviderId::Anthropic);
    }

    #[test]
    fn test_provider_id_deserialization_pascal_case() {
        let fixture_openai: ProviderId = serde_json::from_str(r#""OpenAI""#).unwrap();
        assert_eq!(fixture_openai, ProviderId::OpenAI);

        let fixture_copilot: ProviderId = serde_json::from_str(r#""GithubCopilot""#).unwrap();
        assert_eq!(fixture_copilot, ProviderId::GithubCopilot);
    }
}
