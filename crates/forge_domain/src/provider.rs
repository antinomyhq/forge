use std::collections::HashMap;

use convert_case::Casing;
use derive_more::From;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

use crate::{ApiKey, AuthCredential, AuthDetails, Model, Template};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, JsonSchema)]
#[schemars(transparent)]
pub struct ProviderId(&'static str);

impl ProviderId {
    // Const constructors for built-in providers (snake_case for serialization)
    pub const FORGE: Self = Self("forge");
    pub const OPENAI: Self = Self("openai");
    pub const OPEN_ROUTER: Self = Self("open_router");
    pub const REQUESTY: Self = Self("requesty");
    pub const ZAI: Self = Self("zai");
    pub const ZAI_CODING: Self = Self("zai_coding");
    pub const CEREBRAS: Self = Self("cerebras");
    pub const XAI: Self = Self("xai");
    pub const ANTHROPIC: Self = Self("anthropic");
    pub const CLAUDE_CODE: Self = Self("claude_code");
    pub const VERTEX_AI: Self = Self("vertex_ai");
    pub const BIG_MODEL: Self = Self("big_model");
    pub const AZURE: Self = Self("azure");
    pub const GITHUB_COPILOT: Self = Self("github_copilot");
    pub const OPENAI_COMPATIBLE: Self = Self("openai_compatible");
    pub const ANTHROPIC_COMPATIBLE: Self = Self("anthropic_compatible");

    /// Mapping of snake_case IDs to (const constructor, display name)
    /// This is the single source of truth for all provider metadata.
    const PROVIDER_REGISTRY: &'static [(&'static str, Self, &'static str)] = &[
        ("forge", Self::FORGE, "Forge"),
        ("openai", Self::OPENAI, "OpenAI"),
        ("open_router", Self::OPEN_ROUTER, "OpenRouter"),
        ("requesty", Self::REQUESTY, "Requesty"),
        ("zai", Self::ZAI, "Zai"),
        ("zai_coding", Self::ZAI_CODING, "ZaiCoding"),
        ("cerebras", Self::CEREBRAS, "Cerebras"),
        ("xai", Self::XAI, "Xai"),
        ("anthropic", Self::ANTHROPIC, "Anthropic"),
        ("claude_code", Self::CLAUDE_CODE, "ClaudeCode"),
        ("vertex_ai", Self::VERTEX_AI, "VertexAi"),
        ("big_model", Self::BIG_MODEL, "BigModel"),
        ("azure", Self::AZURE, "Azure"),
        ("github_copilot", Self::GITHUB_COPILOT, "GithubCopilot"),
        (
            "openai_compatible",
            Self::OPENAI_COMPATIBLE,
            "OpenaiCompatible",
        ),
        (
            "anthropic_compatible",
            Self::ANTHROPIC_COMPATIBLE,
            "AnthropicCompatible",
        ),
    ];

    /// Creates a provider ID from a static string reference.
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// Returns the string representation (snake_case, used for serialization
    /// and comparisons).
    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    /// Returns the display name (UpperCamelCase for UI).
    ///
    /// This is used by the `Display` trait for user-facing output.
    pub fn display_name(&self) -> &'static str {
        Self::PROVIDER_REGISTRY
            .iter()
            .find(|(id, _, _)| *id == self.0)
            .map(|(_, _, display)| *display)
            .unwrap_or_else(|| {
                // Fallback for any custom providers (though they're rejected in
                // deserialization)
                Box::leak(
                    self.0
                        .to_case(convert_case::Case::UpperCamel)
                        .into_boxed_str(),
                )
            })
    }

    /// Returns all built-in provider IDs.
    pub fn built_in_providers() -> Vec<Self> {
        Self::PROVIDER_REGISTRY
            .iter()
            .map(|(_, provider, _)| *provider)
            .collect()
    }

    /// Helper to convert from a string to a ProviderId (for deserialization).
    /// Looks up the built-in provider or leaks the string for custom providers.
    fn from_string(s: &str) -> Self {
        Self::PROVIDER_REGISTRY
            .iter()
            .find(|(id, _, _)| *id == s)
            .map(|(_, provider, _)| *provider)
            .unwrap_or_else(|| {
                // For custom providers, leak the string to get a 'static lifetime
                // This is safe because provider IDs are typically loaded once and used
                // throughout
                Self(Box::leak(s.to_string().into_boxed_str()))
            })
    }
}

// Implement FromStr trait
impl std::str::FromStr for ProviderId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_string(s))
    }
}

// Custom Serialize implementation
impl Serialize for ProviderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

// Custom Deserialize implementation
impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        // Validate that it's a known provider
        let provider = Self::from_string(&s);
        Ok(provider)
    }
}

// Display uses display_name() for UI-friendly output
impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
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
        *self == other.0
    }
}

// Enable: provider.id == String::from("openai")
impl PartialEq<String> for ProviderId {
    fn eq(&self, other: &String) -> bool {
        self.0 == other.as_str()
    }
}

// Enable AsRef<str> (returns snake_case for compatibility)
impl AsRef<str> for ProviderId {
    fn as_ref(&self) -> &str {
        self.0
    }
}

// Enable Deref to str (returns snake_case)
impl std::ops::Deref for ProviderId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
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

    /// Gets the authentication methods supported by this provider
    pub fn auth_methods(&self) -> &[crate::AuthMethod] {
        match self {
            AnyProvider::Url(p) => &p.auth_methods,
            AnyProvider::Template(p) => &p.auth_methods,
        }
    }

    /// Consumes self and returns the configured provider if this is a URL
    /// provider with credentials
    pub fn into_configured(self) -> Option<Provider<Url>> {
        match self {
            AnyProvider::Url(p) if p.is_configured() => Some(p),
            _ => None,
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
            id: ProviderId::ZAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::ZAI, key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub(super) fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZAI_CODING,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::ZAI_CODING, key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating an OpenAI provider
    pub(super) fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::OPENAI, key),
            models: Models::Url(Url::parse("https://api.openai.com/v1/models").unwrap()),
        }
    }

    /// Test helper for creating an XAI provider
    pub(super) fn xai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::XAI,
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::XAI, key),
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
            id: ProviderId::VERTEX_AI,
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["project_id", "location"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::VERTEX_AI, key),
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
            id: ProviderId::AZURE,
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["resource_name", "deployment_name", "api_version"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::AZURE, key),
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
            id: ProviderId::XAI,
            response: ProviderResponse::OpenAI,
            url: Url::from_str("https://api.x.ai/v1/chat/completions").unwrap(),
            credential: Some(AuthCredential {
                id: ProviderId::XAI,
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
        assert_eq!(fixture_xai.id, ProviderId::XAI);

        let fixture_other = openai("key");
        assert_ne!(fixture_other.id, ProviderId::XAI);
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

        assert_eq!(fixture.id, ProviderId::AZURE);
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
