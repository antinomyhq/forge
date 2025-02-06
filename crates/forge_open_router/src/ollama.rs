use serde::{Deserialize, Serialize};
use forge_domain::{ChatCompletionMessage, FinishReason};
use crate::provider_kind::ProviderKind;

#[derive(Debug, Default, Clone)] 
pub struct Ollama;

impl ProviderKind for Ollama {
    fn to_chat_completion_message(&self, input: &[u8]) -> anyhow::Result<ChatCompletionMessage> {
        let response: OllamaResponseChunk = serde_json::from_slice(input)?;
        Ok(response.into())
    }

    fn default_base_url(&self) -> String {
        "http://localhost:11434/v1/".to_string()
    }
}

/// Represents an Ollama API response chunk
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OllamaResponseChunk {
    pub choices: Vec<OllamaChoice>,
    pub created: Option<u64>,
    pub id: Option<String>,
    pub model: Option<String>,
    pub object: Option<String>,
    #[serde(rename = "system_fingerprint")]
    pub system_fingerprint: Option<String>,
}

/// Represents a choice in the Ollama response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OllamaChoice {
    pub delta: OlamaDelta,
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
    pub index: Option<u32>,
}

/// Represents the delta part of the Ollama response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OlamaDelta {
    pub content: Option<String>,
    pub role: Option<String>,
    #[serde(rename = "tool_calls")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
}

/// Represents a tool call in the Ollama response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OllamaToolCall {
    pub function: OllamaFunction,
    pub id: Option<String>,
    pub index: Option<u32>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
}

/// Represents a function call in the Ollama response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OllamaFunction {
    pub arguments: Option<String>,
    pub name: Option<String>,
}

/// Trait for converting Ollama response to a standard chat completion message
impl From<OllamaResponseChunk> for forge_domain::ChatCompletionMessage {
    fn from(chunk: OllamaResponseChunk) -> Self {
        // Extract the first choice's delta content if available
        let content = chunk.choices.first()
            .and_then(|choice| choice.delta.content.clone())
            .unwrap_or_default();

        forge_domain::ChatCompletionMessage {
            content: Some(forge_domain::Content::full(content)),
            tool_call: vec![],
            finish_reason: chunk.choices.first()
                .and_then(|_| Some(FinishReason::Stop)),
            usage: None,
        }
    }
}