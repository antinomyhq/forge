use std::collections::HashMap;

use derive_more::From;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};
use url::Url;

use crate::{ApiKey, AuthCredential, AuthDetails, Model, Template};

/// --- IMPORTANT ---
/// The order of providers is important because that would be order in which the
/// providers will be resolved
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    EnumIter,
    PartialOrd,
    Ord,
    JsonSchema,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderResponse {
    OpenAI,
    Anthropic,
}

/// Represents the source of models for a provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Models<T> {
    /// Can be a `Url` or a `Template`
    Url(T),
    Hardcoded(Vec<Model>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider<T> {
    pub id: ProviderId,
    pub response: ProviderResponse,
    pub url: T,
    pub models: Models<T>,
    pub auth_methods: Vec<crate::AuthMethod>,
    pub url_params: Vec<crate::URLParam>,
    pub credential: Option<AuthCredential>,
}

impl<T> Provider<T> {
    pub fn is_configured(&self) -> bool {
        self.credential.is_some()
    }
    pub fn models(&self) -> &Models<T> {
        &self.models
    }
}

impl Provider<Url> {
    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn api_key(&self) -> Option<&ApiKey> {
        self.credential
            .as_ref()
            .and_then(|c| match &c.auth_details {
                AuthDetails::ApiKey(key) => Some(key),
                _ => None,
            })
    }
}

/// Enum for viewing providers in listings where both configured and
/// unconfigured.
#[derive(Debug, Clone, PartialEq, From)]
pub enum AnyProvider {
    Url(Provider<Url>),
    Template(Provider<Template<HashMap<crate::URLParam, crate::URLParamValue>>>),
}

impl AnyProvider {
    /// Returns whether this provider is configured
    pub fn is_configured(&self) -> bool {
        match self {
            AnyProvider::Url(p) => p.is_configured(),
            AnyProvider::Template(p) => p.is_configured(),
        }
    }

    pub fn id(&self) -> ProviderId {
        match self {
            AnyProvider::Url(p) => p.id,
            AnyProvider::Template(p) => p.id,
        }
    }

    /// Gets the response type
    pub fn response(&self) -> &ProviderResponse {
        match self {
            AnyProvider::Url(p) => &p.response,
            AnyProvider::Template(p) => &p.response,
        }
    }

    /// Gets the resolved URL if this is a configured provider
    pub fn url(&self) -> Option<&Url> {
        match self {
            AnyProvider::Url(p) => Some(p.url()),
            AnyProvider::Template(_) => None,
        }
    }
    pub fn url_params(&self) -> &[crate::URLParam] {
        match self {
            AnyProvider::Url(p) => &p.url_params,
            AnyProvider::Template(p) => &p.url_params,
        }
    }
}

#[cfg(test)]
mod test_helpers {
    use std::collections::HashMap;

    use super::*;

    fn make_credential(provider_id: ProviderId, key: &str) -> Option<AuthCredential> {
        Some(AuthCredential {
            id: provider_id,
            auth_details: AuthDetails::ApiKey(ApiKey::from(key.to_string())),
            url_params: HashMap::new(),
        })
    }

    /// Test helper for creating a ZAI provider
    pub(super) fn zai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::Zai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::Zai, key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub(super) fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZaiCoding,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::ZaiCoding, key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating an OpenAI provider
    pub(super) fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OpenAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::OpenAI, key),
            models: Models::Url(Url::parse("https://api.openai.com/v1/models").unwrap()),
        }
    }

    /// Test helper for creating an XAI provider
    pub(super) fn xai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::Xai,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::Xai, key),
            models: Models::Url(Url::parse("https://api.x.ai/v1/models").unwrap()),
        }
    }

    /// Test helper for creating a Vertex AI provider
    pub(super) fn vertex_ai(key: &str, project_id: &str, location: &str) -> Provider<Url> {
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
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["project_id", "location"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::VertexAi, key),
            models: Models::Url(Url::parse(&model_url).unwrap()),
        }
    }

    /// Test helper for creating an Azure provider
    pub(super) fn azure(
        key: &str,
        resource_name: &str,
        deployment_name: &str,
        api_version: &str,
    ) -> Provider<Url> {
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
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["resource_name", "deployment_name", "api_version"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::Azure, key),
            models: Models::Url(Url::parse(&model_url).unwrap()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
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
            credential: Some(AuthCredential {
                id: ProviderId::Xai,
                auth_details: AuthDetails::ApiKey(ApiKey::from(fixture.to_string())),
                url_params: HashMap::new(),
            }),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
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
}
