use forge_domain::{ChatCompletionMessage, ModelId};
use serde::{Deserialize, Serialize};

use crate::provider_kind::ProviderKind;
use crate::{Ollama, OpenApi};

#[derive(Clone)]
pub enum Model {
    Ollama(Ollama),
    OpenAPI(OpenApi),
}

impl ProviderKind for Model {
    fn to_chat_completion_message(&self, input: &[u8]) -> anyhow::Result<ChatCompletionMessage> {
        match self {
            Model::Ollama(x) => x.to_chat_completion_message(input),
            Model::OpenAPI(x) => x.to_chat_completion_message(input),
        }
    }

    fn default_base_url(&self) -> String {
        match self {
            Model::Ollama(x) => x.default_base_url(),
            Model::OpenAPI(x) => x.default_base_url(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenRouterModel {
    pub id: ModelId,
    pub name: String,
    pub created: u64,
    pub description: Option<String>,
    pub context_length: u64,
    pub architecture: Architecture,
    pub pricing: Pricing,
    pub top_provider: TopProvider,
    pub per_request_limits: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Architecture {
    pub modality: String,
    pub tokenizer: String,
    pub instruct_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Pricing {
    pub prompt: String,
    pub completion: String,
    pub image: String,
    pub request: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TopProvider {
    pub context_length: Option<u64>,
    pub max_completion_tokens: Option<u64>,
    pub is_moderated: bool,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ListModelResponse {
    pub data: Vec<OpenRouterModel>,
}
