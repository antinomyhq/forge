use std::fmt::Display;

use serde::{Deserialize, Serialize};

const OPEN_ROUTER_URL: &str = "https://api.openrouter.io/v1/";
const OPENAI_URL: &str = "https://api.openai.com/v1/";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/";

/// Providers that can be used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Provider {
    OpenRouter,
    OpenAI,
    Anthropic,
}

impl Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::OpenRouter => write!(f, "OpenRouter"),
            Provider::OpenAI => write!(f, "OpenAI"),
            Provider::Anthropic => write!(f, "Anthropic"),
        }
    }
}

impl Provider {
    // detects the active provider from environment variables
    pub fn from_env() -> Option<Self> {
        match (
            std::env::var("FORGE_KEY"),
            std::env::var("OPENROUTER_API_KEY"),
            std::env::var("OPENAI_API_KEY"),
            std::env::var("ANTHROPIC_API_KEY"),
        ) {
            (Ok(_), _, _, _) => {
                // note: if we're using FORGE_KEY, we need FORGE_PROVIDER_URL to be set.
                let provider_url = std::env::var("FORGE_PROVIDER_URL").ok()?;
                Self::from_url(&provider_url)
            }
            (_, Ok(_), _, _) => Some(Self::OpenRouter),
            (_, _, Ok(_), _) => Some(Self::OpenAI),
            (_, _, _, Ok(_)) => Some(Self::Anthropic),
            (Err(_), Err(_), Err(_), Err(_)) => None,
        }
    }

    /// converts the provider to it's base URL
    pub fn to_base_url(&self) -> &str {
        match self {
            Provider::OpenRouter => OPEN_ROUTER_URL,
            Provider::OpenAI => OPENAI_URL,
            Provider::Anthropic => ANTHROPIC_URL,
        }
    }

    /// detects the active provider from base URL
    pub fn from_url(url: &str) -> Option<Self> {
        match url {
            OPENAI_URL => Some(Self::OpenAI),
            OPEN_ROUTER_URL => Some(Self::OpenRouter),
            ANTHROPIC_URL => Some(Self::Anthropic),
            _ => None,
        }
    }
}
