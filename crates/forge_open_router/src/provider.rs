use forge_domain::ChatCompletionMessage;

use crate::{Ollama, OpenRouter, ProviderKind};

#[derive(Clone)]
pub enum Provider {
    Ollama(Ollama),
    OpenRouter(OpenRouter),
}

impl ProviderKind for Provider {
    fn to_chat_completion_message(&self, input: &[u8]) -> anyhow::Result<ChatCompletionMessage> {
        match self {
            Provider::Ollama(x) => x.to_chat_completion_message(input),
            Provider::OpenRouter(x) => x.to_chat_completion_message(input),
        }
    }

    fn default_base_url(&self) -> String {
        match self {
            Provider::Ollama(x) => x.default_base_url(),
            Provider::OpenRouter(x) => x.default_base_url(),
        }
    }
}
