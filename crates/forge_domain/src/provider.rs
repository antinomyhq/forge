use serde::{Deserialize, Serialize};
use url::Url;

/// Providers that can be used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Provider {
    OpenAI { url: Url, key: Option<String> },
    Anthropic { url: Url, key: String },
}

impl Provider {
    /// Sets the OpenAI URL if the provider is an OpenAI compatible provider
    pub fn open_ai_url(&mut self, url: String) {
        match self {
            Provider::OpenAI { url: set_url, .. } => {
                *set_url = Url::parse(&url).unwrap();
            }
            Provider::Anthropic { .. } => {}
        }
    }

    /// Sets the Anthropic URL if the provider is Anthropic
    pub fn anthropic_url(&mut self, url: String) {
        match self {
            Provider::Anthropic { url: set_url, .. } => {
                *set_url = Url::parse(&url).unwrap();
            }
            Provider::OpenAI { .. } => {}
        }
    }

    pub fn antinomy(key: &str) -> Provider {
        Provider::OpenAI {
            url: Url::parse(Provider::ANTINOMY_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn openai(key: &str) -> Provider {
        Provider::OpenAI {
            url: Url::parse(Provider::OPENAI_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn open_router(key: &str) -> Provider {
        Provider::OpenAI {
            url: Url::parse(Provider::OPEN_ROUTER_URL).unwrap(),
            key: Some(key.into()),
        }
    }

    pub fn anthropic(key: &str) -> Provider {
        Provider::Anthropic {
            url: Url::parse(Provider::ANTHROPIC_URL).unwrap(),
            key: key.into(),
        }
    }

    pub fn key(&self) -> Option<&str> {
        match self {
            Provider::OpenAI { key, .. } => key.as_deref(),
            Provider::Anthropic { key, .. } => Some(key),
        }
    }
}

impl Provider {
    pub const OPEN_ROUTER_URL: &str = "https://openrouter.ai/api/v1/";
    pub const OPENAI_URL: &str = "https://api.openai.com/v1/";
    pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/";
    pub const ANTINOMY_URL: &str = "https://antinomy.ai/api/v1/";

    /// Converts the provider to it's base URL
    pub fn to_base_url(&self) -> Url {
        match self {
            Provider::OpenAI { url, .. } => url.clone(),
            Provider::Anthropic { url, .. } => url.clone(),
        }
    }

    pub fn is_antinomy(&self) -> bool {
        match self {
            Provider::OpenAI { url, .. } => url.as_str().starts_with(Self::ANTINOMY_URL),
            Provider::Anthropic { .. } => false,
        }
    }

    pub fn is_open_router(&self) -> bool {
        match self {
            Provider::OpenAI { url, .. } => url.as_str().starts_with(Self::OPEN_ROUTER_URL),
            Provider::Anthropic { .. } => false,
        }
    }

    pub fn is_open_ai(&self) -> bool {
        match self {
            Provider::OpenAI { url, .. } => url.as_str().starts_with(Self::OPENAI_URL),
            Provider::Anthropic { .. } => false,
        }
    }

    pub fn is_anthropic(&self) -> bool {
        match self {
            Provider::OpenAI { .. } => false,
            Provider::Anthropic { url, .. } => url.as_str().starts_with(Self::ANTHROPIC_URL),
        }
    }
}
