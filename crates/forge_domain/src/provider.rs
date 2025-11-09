use std::collections::HashMap;

use derive_more::AsRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{ApiKey, AuthCredential, AuthDetails, Model, Template};

/// --- IMPORTANT ---
/// The order of providers is important because that would be order in which the
/// providers will be resolved
///
/// Provider identifier that supports both built-in and custom providers.
///
/// Built-in providers are available as string constants and constructor
/// methods. Custom providers can be created from strings: `"ollama".into()`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, AsRef, Serialize, Deserialize, JsonSchema,
)]
pub struct ProviderId(String);

impl ProviderId {
    // String constants for built-in providers
    pub const FORGE_STR: &'static str = "forge";
    pub const OPENAI_STR: &'static str = "openai";
    pub const OPEN_ROUTER_STR: &'static str = "open_router";
    pub const REQUESTY_STR: &'static str = "requesty";
    pub const ZAI_STR: &'static str = "zai";
    pub const ZAI_CODING_STR: &'static str = "zai_coding";
    pub const CEREBRAS_STR: &'static str = "cerebras";
    pub const XAI_STR: &'static str = "xai";
    pub const ANTHROPIC_STR: &'static str = "anthropic";
    pub const CLAUDE_CODE_STR: &'static str = "claude_code";
    pub const VERTEX_AI_STR: &'static str = "vertex_ai";
    pub const BIG_MODEL_STR: &'static str = "big_model";
    pub const AZURE_STR: &'static str = "azure";
    pub const GITHUB_COPILOT_STR: &'static str = "github_copilot";
    pub const OPENAI_COMPATIBLE_STR: &'static str = "openai_compatible";
    pub const ANTHROPIC_COMPATIBLE_STR: &'static str = "anthropic_compatible";

    /// Creates a Forge provider ID
    pub fn forge() -> Self {
        Self(Self::FORGE_STR.to_string())
    }

    /// Creates an OpenAI provider ID
    pub fn openai() -> Self {
        Self(Self::OPENAI_STR.to_string())
    }

    /// Creates an OpenRouter provider ID
    pub fn open_router() -> Self {
        Self(Self::OPEN_ROUTER_STR.to_string())
    }

    /// Creates a Requesty provider ID
    pub fn requesty() -> Self {
        Self(Self::REQUESTY_STR.to_string())
    }

    /// Creates a ZAI provider ID
    pub fn zai() -> Self {
        Self(Self::ZAI_STR.to_string())
    }

    /// Creates a ZAI Coding provider ID
    pub fn zai_coding() -> Self {
        Self(Self::ZAI_CODING_STR.to_string())
    }

    /// Creates a Cerebras provider ID
    pub fn cerebras() -> Self {
        Self(Self::CEREBRAS_STR.to_string())
    }

    /// Creates an XAI provider ID
    pub fn xai() -> Self {
        Self(Self::XAI_STR.to_string())
    }

    /// Creates an Anthropic provider ID
    pub fn anthropic() -> Self {
        Self(Self::ANTHROPIC_STR.to_string())
    }

    /// Creates a Claude Code provider ID
    pub fn claude_code() -> Self {
        Self(Self::CLAUDE_CODE_STR.to_string())
    }

    /// Creates a Vertex AI provider ID
    pub fn vertex_ai() -> Self {
        Self(Self::VERTEX_AI_STR.to_string())
    }

    /// Creates a BigModel provider ID
    pub fn big_model() -> Self {
        Self(Self::BIG_MODEL_STR.to_string())
    }

    /// Creates an Azure provider ID
    pub fn azure() -> Self {
        Self(Self::AZURE_STR.to_string())
    }

    /// Creates a GitHub Copilot provider ID
    pub fn github_copilot() -> Self {
        Self(Self::GITHUB_COPILOT_STR.to_string())
    }

    /// Creates an OpenAI Compatible provider ID
    pub fn openai_compatible() -> Self {
        Self(Self::OPENAI_COMPATIBLE_STR.to_string())
    }

    /// Creates an Anthropic Compatible provider ID
    pub fn anthropic_compatible() -> Self {
        Self(Self::ANTHROPIC_COMPATIBLE_STR.to_string())
    }

    /// Returns true if this is a built-in provider
    pub fn is_built_in(&self) -> bool {
        matches!(
            self.0.as_ref(),
            Self::FORGE_STR
                | Self::OPENAI_STR
                | Self::OPEN_ROUTER_STR
                | Self::REQUESTY_STR
                | Self::ZAI_STR
                | Self::ZAI_CODING_STR
                | Self::CEREBRAS_STR
                | Self::XAI_STR
                | Self::ANTHROPIC_STR
                | Self::CLAUDE_CODE_STR
                | Self::VERTEX_AI_STR
                | Self::BIG_MODEL_STR
                | Self::AZURE_STR
                | Self::GITHUB_COPILOT_STR
                | Self::OPENAI_COMPATIBLE_STR
                | Self::ANTHROPIC_COMPATIBLE_STR
        )
    }

    /// Returns all built-in provider IDs
    pub fn built_in_providers() -> Vec<Self> {
        vec![
            Self::forge(),
            Self::openai(),
            Self::open_router(),
            Self::requesty(),
            Self::zai(),
            Self::zai_coding(),
            Self::cerebras(),
            Self::xai(),
            Self::anthropic(),
            Self::claude_code(),
            Self::vertex_ai(),
            Self::big_model(),
            Self::azure(),
            Self::github_copilot(),
            Self::openai_compatible(),
            Self::anthropic_compatible(),
        ]
    }
}

// ⚠️ Only manual implementations needed (derive_more doesn't support these)
// Enable: ProviderId::from("custom_provider")
impl From<String> for ProviderId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// Enable: ProviderId::from("custom_provider")
impl From<&str> for ProviderId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// Enable: provider.id == "openai"
impl PartialEq<&str> for ProviderId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

// Enable: "openai" == provider.id
impl PartialEq<ProviderId> for &str {
    fn eq(&self, other: &ProviderId) -> bool {
        *self == other.0.as_str()
    }
}

// Enable: provider.id == String::from("openai")
impl PartialEq<String> for ProviderId {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

// Display shows the raw string value (e.g., "openai", not "OpenAI")
impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
#[derive(Debug, Clone, PartialEq)]
pub enum AnyProvider {
    Url(Provider<Url>),
    Template(Provider<Template<HashMap<crate::URLParam, crate::URLParamValue>>>),
}

impl From<Provider<Url>> for AnyProvider {
    fn from(p: Provider<Url>) -> Self {
        Self::Url(p)
    }
}

impl From<Provider<Template<HashMap<crate::URLParam, crate::URLParamValue>>>> for AnyProvider {
    fn from(p: Provider<Template<HashMap<crate::URLParam, crate::URLParamValue>>>) -> Self {
        Self::Template(p)
    }
}

impl AnyProvider {
    /// Returns whether this provider is configured
    pub fn is_configured(&self) -> bool {
        match self {
            AnyProvider::Url(p) => p.is_configured(),
            AnyProvider::Template(p) => p.is_configured(),
        }
    }

    pub fn id(&self) -> &ProviderId {
        match self {
            AnyProvider::Url(p) => &p.id,
            AnyProvider::Template(p) => &p.id,
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
            id: ProviderId::zai(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::zai(), key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub(super) fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::zai_coding(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::zai_coding(), key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating an OpenAI provider
    pub(super) fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::openai(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::openai(), key),
            models: Models::Url(Url::parse("https://api.openai.com/v1/models").unwrap()),
        }
    }

    /// Test helper for creating an XAI provider
    pub(super) fn xai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::xai(),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::xai(), key),
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
            id: ProviderId::vertex_ai(),
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["project_id", "location"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::vertex_ai(), key),
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
            id: ProviderId::azure(),
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["resource_name", "deployment_name", "api_version"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::azure(), key),
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
            id: ProviderId::xai(),
            response: ProviderResponse::OpenAI,
            url: Url::from_str("https://api.x.ai/v1/chat/completions").unwrap(),
            credential: Some(AuthCredential {
                id: ProviderId::xai(),
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
        assert_eq!(fixture_xai.id, ProviderId::xai());

        let fixture_other = openai("key");
        assert_ne!(fixture_other.id, ProviderId::xai());
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

        assert_eq!(fixture.id, ProviderId::azure());
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
