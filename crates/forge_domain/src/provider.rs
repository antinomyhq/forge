use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use derive_more::From;
use schemars::JsonSchema;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize};
use strum_macros::EnumIter;
use url::Url;

use crate::{ApiKey, AuthCredential, AuthDetails, Model, Template};

/// --- IMPORTANT ---
/// Built-in provider order is important because that would be order in which
/// the built-in providers will be resolved
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    PartialOrd,
    Ord,
    JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInProviderId {
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
    ClaudeCode,
    VertexAi,
    BigModel,
    Azure,
    GithubCopilot,
    #[serde(rename = "openai_compatible")]
    OpenAICompatible,
    AnthropicCompatible,
}

impl fmt::Display for BuiltInProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BuiltInProviderId::Forge => "forge",
            BuiltInProviderId::OpenAI => "openai",
            BuiltInProviderId::OpenRouter => "open_router",
            BuiltInProviderId::Requesty => "requesty",
            BuiltInProviderId::Zai => "zai",
            BuiltInProviderId::ZaiCoding => "zai_coding",
            BuiltInProviderId::Cerebras => "cerebras",
            BuiltInProviderId::Xai => "xai",
            BuiltInProviderId::Anthropic => "anthropic",
            BuiltInProviderId::ClaudeCode => "claude_code",
            BuiltInProviderId::VertexAi => "vertex_ai",
            BuiltInProviderId::BigModel => "big_model",
            BuiltInProviderId::Azure => "azure",
            BuiltInProviderId::GithubCopilot => "github_copilot",
            BuiltInProviderId::OpenAICompatible => "openai_compatible",
            BuiltInProviderId::AnthropicCompatible => "anthropic_compatible",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for BuiltInProviderId {
    type Err = ProviderIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "forge" => Ok(BuiltInProviderId::Forge),
            "openai" => Ok(BuiltInProviderId::OpenAI),
            "open_router" => Ok(BuiltInProviderId::OpenRouter),
            "requesty" => Ok(BuiltInProviderId::Requesty),
            "zai" => Ok(BuiltInProviderId::Zai),
            "zai_coding" => Ok(BuiltInProviderId::ZaiCoding),
            "cerebras" => Ok(BuiltInProviderId::Cerebras),
            "xai" => Ok(BuiltInProviderId::Xai),
            "anthropic" => Ok(BuiltInProviderId::Anthropic),
            "claude_code" => Ok(BuiltInProviderId::ClaudeCode),
            "vertex_ai" => Ok(BuiltInProviderId::VertexAi),
            "big_model" => Ok(BuiltInProviderId::BigModel),
            "azure" => Ok(BuiltInProviderId::Azure),
            "github_copilot" => Ok(BuiltInProviderId::GithubCopilot),
            "openai_compatible" => Ok(BuiltInProviderId::OpenAICompatible),
            "anthropic_compatible" => Ok(BuiltInProviderId::AnthropicCompatible),
            _ => Err(ProviderIdError::UnknownBuiltIn(s.to_string())),
        }
    }
}

/// Provider identifier that supports both built-in and custom providers
#[derive(Debug, Clone, PartialEq, Eq, Hash, JsonSchema, PartialOrd, Ord)]
pub enum ProviderId {
    /// Built-in provider with compile-time type safety
    BuiltIn(BuiltInProviderId),
    /// Custom provider with runtime-defined name
    Custom(String),
}

impl ProviderId {
    /// Check if this is a built-in provider
    pub fn is_builtin(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(_))
    }

    /// Check if this is a custom provider
    pub fn is_custom(&self) -> bool {
        matches!(self, ProviderId::Custom(_))
    }

    /// Get the string representation of this provider ID
    pub fn as_str(&self) -> String {
        match self {
            ProviderId::BuiltIn(builtin) => builtin.to_string(),
            ProviderId::Custom(name) => name.clone(),
        }
    }

    /// Convert to built-in provider ID if possible
    pub fn as_builtin(&self) -> Option<BuiltInProviderId> {
        match self {
            ProviderId::BuiltIn(builtin) => Some(*builtin),
            ProviderId::Custom(_) => None,
        }
    }

    /// Convert to custom provider name if possible
    pub fn as_custom(&self) -> Option<&str> {
        match self {
            ProviderId::BuiltIn(_) => None,
            ProviderId::Custom(name) => Some(name),
        }
    }

    /// Create a custom provider ID with validation
    pub fn custom(name: String) -> Result<Self, ProviderIdError> {
        if name.trim().is_empty() {
            return Err(ProviderIdError::EmptyName);
        }

        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ProviderIdError::InvalidName(name));
        }

        Ok(ProviderId::Custom(name))
    }

    /// Check if provider ID matches a built-in provider
    pub fn is_cerebras(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::Cerebras))
    }

    /// Check if provider ID matches ZAI
    pub fn is_zai(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::Zai))
    }

    /// Check if provider ID matches ZAI Coding
    pub fn is_zai_coding(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::ZaiCoding))
    }

    /// Check if provider ID matches OpenRouter
    pub fn is_open_router(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::OpenRouter))
    }

    /// Check if provider ID matches Forge
    pub fn is_forge(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::Forge))
    }

    /// Check if provider ID matches OpenAI
    pub fn is_openai(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::OpenAI))
    }

    /// Check if provider ID matches XAI
    pub fn is_xai(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::Xai))
    }

    /// Check if provider ID matches Anthropic
    pub fn is_anthropic(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::Anthropic))
    }

    /// Check if provider ID matches Claude Code
    pub fn is_claude_code(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::ClaudeCode))
    }

    /// Check if provider ID matches Vertex AI
    pub fn is_vertex_ai(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::VertexAi))
    }

    /// Check if provider ID matches BigModel
    pub fn is_big_model(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::BigModel))
    }

    /// Check if provider ID matches Azure
    pub fn is_azure(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::Azure))
    }

    /// Check if provider ID matches Github Copilot
    pub fn is_github_copilot(&self) -> bool {
        matches!(self, ProviderId::BuiltIn(BuiltInProviderId::GithubCopilot))
    }

    /// Check if provider ID matches OpenAI Compatible
    pub fn is_openai_compatible(&self) -> bool {
        matches!(
            self,
            ProviderId::BuiltIn(BuiltInProviderId::OpenAICompatible)
        )
    }

    /// Check if provider ID matches Anthropic Compatible
    pub fn is_anthropic_compatible(&self) -> bool {
        matches!(
            self,
            ProviderId::BuiltIn(BuiltInProviderId::AnthropicCompatible)
        )
    }

    /// Create a built-in Forge provider ID
    pub fn forge() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::Forge)
    }

    /// Create a built-in OpenAI provider ID
    pub fn openai() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::OpenAI)
    }

    /// Create a built-in ZAI provider ID
    pub fn zai() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::Zai)
    }

    /// Create a built-in ZAI Coding provider ID
    pub fn zai_coding() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::ZaiCoding)
    }

    /// Create a built-in Open Router provider ID
    pub fn open_router() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::OpenRouter)
    }

    /// Create a built-in XAI provider ID
    pub fn xai() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::Xai)
    }

    /// Create a built-in Anthropic provider ID
    pub fn anthropic() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::Anthropic)
    }

    /// Create a built-in Vertex AI provider ID
    pub fn vertex_ai() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::VertexAi)
    }

    /// Create a built-in Azure provider ID
    pub fn azure() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::Azure)
    }

    /// Create a built-in Github Copilot provider ID
    pub fn github_copilot() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::GithubCopilot)
    }

    /// Create a built-in OpenAI Compatible provider ID
    pub fn openai_compatible() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::OpenAICompatible)
    }

    /// Create a built-in Cerebras provider ID
    pub fn cerebras() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::Cerebras)
    }

    /// Create a built-in Big Model provider ID
    pub fn big_model() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::BigModel)
    }

    /// Create a built-in Anthropic Compatible provider ID
    pub fn anthropic_compatible() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::AnthropicCompatible)
    }

    /// Create a built-in Requesty provider ID
    pub fn requesty() -> Self {
        ProviderId::BuiltIn(BuiltInProviderId::Requesty)
    }

    /// Iterate over all built-in provider IDs
    pub fn iter() -> impl Iterator<Item = Self> {
        [
            BuiltInProviderId::Forge,
            BuiltInProviderId::OpenAI,
            BuiltInProviderId::OpenRouter,
            BuiltInProviderId::Requesty,
            BuiltInProviderId::Zai,
            BuiltInProviderId::ZaiCoding,
            BuiltInProviderId::Cerebras,
            BuiltInProviderId::Xai,
            BuiltInProviderId::Anthropic,
            BuiltInProviderId::VertexAi,
            BuiltInProviderId::BigModel,
            BuiltInProviderId::Azure,
            BuiltInProviderId::GithubCopilot,
            BuiltInProviderId::OpenAICompatible,
            BuiltInProviderId::AnthropicCompatible,
        ]
        .into_iter()
        .map(ProviderId::BuiltIn)
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderId::BuiltIn(builtin) => write!(f, "{}", builtin),
            ProviderId::Custom(name) => write!(f, "{}", name),
        }
    }
}

impl FromStr for ProviderId {
    type Err = ProviderIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // First try to parse as built-in provider
        if let Ok(builtin) = BuiltInProviderId::from_str(s) {
            return Ok(ProviderId::BuiltIn(builtin));
        }

        // If not built-in, treat as custom provider
        ProviderId::custom(s.to_string())
    }
}

impl Serialize for ProviderId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Always serialize as string for backward compatibility with config files
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ProviderId::from_str(&s)
            .map_err(|e| D::Error::custom(format!("Invalid provider ID: {}", e)))
    }
}

/// Errors that can occur when creating or parsing provider IDs
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderIdError {
    #[error("Provider name cannot be empty")]
    EmptyName,
    #[error(
        "Invalid provider name: '{0}'. Only alphanumeric characters, underscores, and hyphens are allowed"
    )]
    InvalidName(String),
    #[error("Unknown built-in provider: '{0}'")]
    UnknownBuiltIn(String),
}

// Convert from BuiltInProviderId to ProviderId for convenience
impl From<BuiltInProviderId> for ProviderId {
    fn from(builtin: BuiltInProviderId) -> Self {
        ProviderId::BuiltIn(builtin)
    }
}

// Convert from &str to ProviderId for convenience
impl TryFrom<&str> for ProviderId {
    type Error = ProviderIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        ProviderId::from_str(value)
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
            AnyProvider::Url(p) => p.id.clone(),
            AnyProvider::Template(p) => p.id.clone(),
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
            id: ProviderId::BuiltIn(BuiltInProviderId::Zai),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::BuiltIn(BuiltInProviderId::Zai), key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating a ZAI Coding provider
    pub(super) fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::BuiltIn(BuiltInProviderId::ZaiCoding),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::BuiltIn(BuiltInProviderId::ZaiCoding), key),
            models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
        }
    }

    /// Test helper for creating an OpenAI provider
    pub(super) fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::BuiltIn(BuiltInProviderId::OpenAI),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::BuiltIn(BuiltInProviderId::OpenAI), key),
            models: Models::Url(Url::parse("https://api.openai.com/v1/models").unwrap()),
        }
    }

    /// Test helper for creating an XAI provider
    pub(super) fn xai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::BuiltIn(BuiltInProviderId::Xai),
            response: ProviderResponse::OpenAI,
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::BuiltIn(BuiltInProviderId::Xai), key),
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
            id: ProviderId::BuiltIn(BuiltInProviderId::VertexAi),
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["project_id", "location"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::BuiltIn(BuiltInProviderId::VertexAi), key),
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
            id: ProviderId::BuiltIn(BuiltInProviderId::Azure),
            response: ProviderResponse::OpenAI,
            url: Url::parse(&chat_url).unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: ["resource_name", "deployment_name", "api_version"]
                .iter()
                .map(|&s| s.to_string().into())
                .collect(),
            credential: make_credential(ProviderId::BuiltIn(BuiltInProviderId::Azure), key),
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
            id: ProviderId::BuiltIn(BuiltInProviderId::Xai),
            response: ProviderResponse::OpenAI,
            url: Url::from_str("https://api.x.ai/v1/chat/completions").unwrap(),
            credential: Some(AuthCredential {
                id: ProviderId::BuiltIn(BuiltInProviderId::Xai),
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
        assert_eq!(fixture_xai.id, ProviderId::BuiltIn(BuiltInProviderId::Xai));

        let fixture_other = openai("key");
        assert_ne!(
            fixture_other.id,
            ProviderId::BuiltIn(BuiltInProviderId::Xai)
        );
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

        assert_eq!(fixture.id, ProviderId::BuiltIn(BuiltInProviderId::Azure));
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
    fn test_custom_provider_id_creation() {
        let custom_id = ProviderId::custom("vllmlocal".to_string()).unwrap();
        assert!(custom_id.is_custom());
        assert!(!custom_id.is_builtin());
        assert_eq!(custom_id.to_string(), "vllmlocal");
        assert_eq!(custom_id.as_custom(), Some("vllmlocal"));
        assert_eq!(custom_id.as_builtin(), None);
    }

    #[test]
    fn test_builtin_provider_id_conversion() {
        let builtin = ProviderId::BuiltIn(BuiltInProviderId::OpenAI);
        assert!(builtin.is_builtin());
        assert!(!builtin.is_custom());
        assert_eq!(builtin.to_string(), "openai");
        assert_eq!(builtin.as_builtin(), Some(BuiltInProviderId::OpenAI));
        assert_eq!(builtin.as_custom(), None);
    }

    #[test]
    fn test_provider_id_from_str_builtin() {
        let provider_id = ProviderId::from_str("openai").unwrap();
        assert!(matches!(
            provider_id,
            ProviderId::BuiltIn(BuiltInProviderId::OpenAI)
        ));
    }

    #[test]
    fn test_provider_id_from_str_custom() {
        let provider_id = ProviderId::from_str("vllmlocal").unwrap();
        assert!(matches!(provider_id, ProviderId::Custom(name) if name == "vllmlocal"));
    }

    #[test]
    fn test_provider_id_validation() {
        // Valid names
        assert!(ProviderId::custom("valid_name".to_string()).is_ok());
        assert!(ProviderId::custom("valid-name".to_string()).is_ok());
        assert!(ProviderId::custom("valid_name123".to_string()).is_ok());

        // Invalid names
        assert!(matches!(
            ProviderId::custom("".to_string()),
            Err(ProviderIdError::EmptyName)
        ));
        assert!(matches!(
            ProviderId::custom("invalid name".to_string()),
            Err(ProviderIdError::InvalidName(_))
        ));
        assert!(matches!(
            ProviderId::custom("invalid@name".to_string()),
            Err(ProviderIdError::InvalidName(_))
        ));
    }

    #[test]
    fn test_provider_id_from_builtin() {
        let builtin = BuiltInProviderId::OpenAI;
        let provider_id: ProviderId = builtin.into();
        assert_eq!(provider_id.to_string(), "openai");
        assert!(provider_id.is_builtin());
    }
}
