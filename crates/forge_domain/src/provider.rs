use derive_more::Display;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use url::Url;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
pub enum Provider {
    OpenAI(ProviderDetails),
    Anthropic(ProviderDetails),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Display)]
#[display("{} ({})", name, id)]
pub struct ProviderDetails {
    pub id: String,
    pub name: String,
    pub description: String,
    pub api_key: String,
    pub provider_type: String, // Type of provider (e.g., "openai", "anthropic")
    pub base_url: String,
}

/// Configuration for multiple providers
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProviderConfig {
    pub provider_id: Option<String>,  // ID of the currently active provider
    pub providers: Vec<ProviderDetails>,
}

impl ProviderConfig {
    pub fn new(provider_id: Option<String>, providers: Vec<ProviderDetails>) -> Self {
        Self {
            provider_id,
            providers,
        }
    }

    pub fn get_provider(&self) -> Result<Provider> {
        Provider::new(self)
    }

    pub fn get_providers(&self) -> &Vec<ProviderDetails> {
        &self.providers
    }

    /// Set the active provider by ID
    pub fn set_active_provider(&mut self, provider_id: String) {
        self.provider_id = Some(provider_id);
    }

    /// Get the ID of a provider
    pub fn get_provider_id(provider: &Provider) -> &str {
        match provider {
            Provider::OpenAI(details) => &details.id,
            Provider::Anthropic(details) => &details.id,
        }
    }

    /// Create a default provider configuration with common providers
    pub fn default() -> Vec<ProviderDetails> {
        vec![
            ProviderDetails::new(
                "openai".to_string(),
                "OpenAI".to_string(),
                "OpenAI API provider".to_string(),
                "OPENAI_API_KEY".to_string(),
                "openai".to_string(),
                "https://api.openai.com/v1".to_string(),
            ),
            ProviderDetails::new(
                "anthropic".to_string(),
                "Anthropic".to_string(),
                "Anthropic API provider".to_string(),
                "ANTHROPIC_API_KEY".to_string(),
                "anthropic".to_string(),
                "https://api.anthropic.com/v1".to_string(),
            ),
            ProviderDetails::new(
                "forge".to_string(),
                "Forge".to_string(),
                "Forge API provider".to_string(),
                "FORGE_KEY".to_string(),
                "openai".to_string(),
                "https://antinomy.ai/api/v1".to_string(),
            ),
            ProviderDetails::new(
                "openrouter".to_string(),
                "OpenRouter".to_string(),
                "OpenRouter API provider".to_string(),
                "OPENROUTER_API_KEY".to_string(),
                "openai".to_string(),
                "https://openrouter.ai/api/v1".to_string(),
            ),
            ProviderDetails::new(
                "requesty".to_string(),
                "Requesty".to_string(),
                "Requesty API provider".to_string(),
                "REQUESTY_API_KEY".to_string(),
                "openai".to_string(),
                "https://requesty.ai/api/v1".to_string(),
            ),
        ]
    }
}

impl Provider {
    pub fn new(provider_config: &ProviderConfig) -> Result<Self> {
        let provider_id = provider_config.provider_id.as_ref()
            .ok_or_else(|| anyhow!("No active provider ID set"))?;

        let provider_details = provider_config.providers
            .iter()
            .find(|p| &p.id == provider_id)
            .ok_or_else(|| anyhow!("Provider ID '{}' not found in providers list", provider_id))?;

        let provider = provider_details.provider()?;
        Ok(provider)
    }

    pub fn to_base_url(&self) -> Url {
        match self {
            Provider::OpenAI(details) => Url::parse(&details.base_url).expect("Invalid OpenAI URL"),
            Provider::Anthropic(details) => Url::parse(&details.base_url).expect("Invalid Anthropic URL"),
        }
    }

    pub fn key(&self) -> Option<&str> {
        match self {
            Provider::OpenAI(details) => Some(&details.api_key),
            Provider::Anthropic(details) => Some(&details.api_key),
        }
    }

    pub fn get_base_url(&self) -> &str {
        match self {
            Provider::OpenAI(details) => &details.base_url,
            Provider::Anthropic(details) => &details.base_url,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Provider::OpenAI(details) => &details.id,
            Provider::Anthropic(details) => &details.id,
        }
    }

    pub fn base_url(&self) -> &str {
        match self {
            Provider::OpenAI(details) => &details.base_url,
            Provider::Anthropic(details) => &details.base_url,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Provider::OpenAI(details) => &details.name,
            Provider::Anthropic(details) => &details.name,
        }
    }

    pub fn api_key(&self) -> &str {
        match self {
            Provider::OpenAI(details) => &details.api_key,
            Provider::Anthropic(details) => &details.api_key,
        }
    }
}

impl ProviderDetails {
    pub fn new (
        id: String,
        name: String,
        description: String,
        api_key: String,
        provider_type: String,
        base_url: String,
    ) -> Self {
        Self {
            id,
            name,
            description,
            api_key,
            provider_type,
            base_url: if base_url.ends_with('/') { base_url } else { format!("{}/", base_url) },
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn provider(&self) -> Result<Provider> {
        match self.provider_type.as_str() {
            "openai" => Ok(Provider::OpenAI(self.clone())),
            "anthropic" => Ok(Provider::Anthropic(self.clone())),
            _ => Err(anyhow!("Unknown provider type: {}", self.provider_type)),
        }
    }
}