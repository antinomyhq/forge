use std::str::FromStr;
use forge_domain::ChatCompletionMessage;
use crate::{Ollama, OpenRouter, ProviderKind};

#[derive(Clone)]
pub enum Provider {
    Ollama(Ollama),
    OpenAPI(OpenRouter),
}

impl FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match s.as_str() {
            "ollama" => Ok(Provider::Ollama(Ollama::default())),
            "openapi" => Ok(Provider::OpenAPI(OpenRouter::default())),
            _ => anyhow::bail!("Invalid model type: {}", s),
        }
    }
}

impl ProviderKind for Provider {
    fn to_chat_completion_message(&self, input: &[u8]) -> anyhow::Result<ChatCompletionMessage> {
        match self {
            Provider::Ollama(x) => x.to_chat_completion_message(input),
            Provider::OpenAPI(x) => x.to_chat_completion_message(input),
        }
    }

    fn default_base_url(&self) -> String {
        match self {
            Provider::Ollama(x) => x.default_base_url(),
            Provider::OpenAPI(x) => x.default_base_url(),
        }
    }
}
