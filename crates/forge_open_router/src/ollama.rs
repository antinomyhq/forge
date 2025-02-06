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
impl From<OllamaResponseChunk> for ChatCompletionMessage {
    fn from(chunk: OllamaResponseChunk) -> Self {
        let content = chunk.choices.first()
            .and_then(|choice| choice.delta.content.clone())
            .unwrap_or_default();

        // Convert Ollama tool calls to domain tool calls
        let tool_calls = chunk.choices.iter()
            .filter_map(|choice| choice.delta.tool_calls.as_ref())
            .flat_map(|tool_calls| {
                tool_calls.iter().map(|tool_call| {
                    forge_domain::ToolCall::Part(forge_domain::ToolCallPart {
                        call_id: tool_call.id.as_ref()
                            .map(|id| forge_domain::ToolCallId::new(id.clone())),
                        name: tool_call.function.name.as_ref()
                            .map(|name| forge_domain::ToolName::new(name.clone())),
                        arguments_part: tool_call.function.arguments.clone().unwrap_or_default(),
                    })
                }).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        forge_domain::ChatCompletionMessage {
            content: Some(forge_domain::Content::full(content)),
            tool_call: tool_calls,
            finish_reason: chunk.choices.first()
                .and_then(|choice| match choice.finish_reason.as_deref() {
                    Some("tool_calls") => Some(FinishReason::ToolCalls),
                    Some("stop") => Some(FinishReason::Stop),
                    _ => None
                }),
            usage: None,
        }
    }
}
