use derive_setters::Setters;
use forge_app::domain::{ContextMessage, Image};
use serde::{Deserialize, Serialize};

use super::super::error::Error;

#[derive(Serialize, Default, Setters)]
#[setters(into, strip_option)]
pub struct Request {
    max_tokens: u64,
    messages: Vec<Message>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<Thinking>,
}

#[derive(Serialize, Default)]
pub struct Thinking {
    r#type: String,
    budget_tokens: u64,
}

impl TryFrom<forge_app::domain::Context> for Request {
    type Error = anyhow::Error;
    fn try_from(request: forge_app::domain::Context) -> std::result::Result<Self, Self::Error> {
        // note: Anthropic only supports 1 system message in context, so from the
        // context we pick the first system message available.
        // ref: https://docs.anthropic.com/en/api/messages#body-system
        let system = request.messages.iter().find_map(|message| {
            if let ContextMessage::Text(chat_message) = message {
                if chat_message.role == forge_app::domain::Role::System {
                    Some(chat_message.content.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });

        Ok(Self {
            messages: request
                .messages
                .into_iter()
                .filter(|message| {
                    // note: Anthropic does not support system messages in message field.
                    if let ContextMessage::Text(chat_message) = message {
                        chat_message.role != forge_app::domain::Role::System
                    } else {
                        true
                    }
                })
                .map(Message::try_from)
                .collect::<std::result::Result<Vec<_>, _>>()?,
            tools: request
                .tools
                .into_iter()
                .map(ToolDefinition::try_from)
                .collect::<std::result::Result<Vec<_>, _>>()?,
            system,
            temperature: request.temperature.map(|t| t.value()),
            top_p: request.top_p.map(|t| t.value()),
            top_k: request.top_k.map(|t| t.value() as u64),
            tool_choice: request.tool_choice.map(ToolChoice::from),
            thinking: request.reasoning.and_then(|reasoning| {
                match (reasoning.enabled, reasoning.max_tokens) {
                    (Some(true), Some(max_tokens)) => Some(Thinking {
                        r#type: "enabled".to_string(),
                        budget_tokens: max_tokens as u64,
                    }),
                    _ => None,
                }
            }),
            ..Default::default()
        })
    }
}

#[derive(Serialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
}

#[derive(Serialize)]
pub struct Message {
    content: Vec<Content>,
    role: Role,
}

impl TryFrom<ContextMessage> for Message {
    type Error = anyhow::Error;
    fn try_from(value: ContextMessage) -> std::result::Result<Self, Self::Error> {
        Ok(match value {
            ContextMessage::Text(chat_message) => {
                let mut content = Vec::with_capacity(
                    chat_message
                        .tool_calls
                        .as_ref()
                        .map(|tc| tc.len())
                        .unwrap_or_default()
                        + 1,
                );

                if let Some(reasoning) = chat_message.reasoning_details
                    && let Some((sig, text)) = reasoning.into_iter().find_map(|reasoning| {
                        match (reasoning.signature, reasoning.text) {
                            (Some(sig), Some(text)) => Some((sig, text)),
                            _ => None,
                        }
                    })
                {
                    content.push(Content::Thinking { signature: Some(sig), thinking: Some(text) });
                }

                if !chat_message.content.is_empty() {
                    // note: Anthropic does not allow empty text content.
                    content.push(Content::Text { text: chat_message.content, cache_control: None });
                }
                if let Some(tool_calls) = chat_message.tool_calls {
                    for tool_call in tool_calls {
                        content.push(tool_call.try_into()?);
                    }
                }

                match chat_message.role {
                    forge_app::domain::Role::User => Message { role: Role::User, content },
                    forge_app::domain::Role::Assistant => {
                        Message { role: Role::Assistant, content }
                    }
                    forge_app::domain::Role::System => {
                        // note: Anthropic doesn't support system role messages and they're already
                        // filtered out. so this state is unreachable.
                        return Err(Error::UnsupportedRole("System".to_string()).into());
                    }
                }
            }
            ContextMessage::Tool(tool_result) => {
                Message { role: Role::User, content: vec![tool_result.try_into()?] }
            }
            ContextMessage::Image(img) => {
                Message { content: vec![Content::from(img)], role: Role::User }
            }
        })
    }
}

impl From<Image> for Content {
    fn from(value: Image) -> Self {
        Content::Image {
            source: ImageSource {
                type_: "url".to_string(),
                media_type: None,
                data: None,
                url: Some(value.url().clone()),
            },
        }
    }
}

#[derive(Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum Content {
    Image {
        source: ImageSource,
    },
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolUse {
        id: String,
        input: Option<serde_json::Value>,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Thinking {
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking: Option<String>,
    },
}

impl TryFrom<forge_app::domain::ToolCallFull> for Content {
    type Error = anyhow::Error;
    fn try_from(value: forge_app::domain::ToolCallFull) -> std::result::Result<Self, Self::Error> {
        let call_id = value.call_id.as_ref().ok_or(Error::ToolCallMissingId)?;

        Ok(Content::ToolUse {
            id: call_id.as_str().to_string(),
            input: serde_json::to_value(value.arguments).ok(),
            name: value.name.to_string(),
            cache_control: None,
        })
    }
}

impl TryFrom<forge_app::domain::ToolResult> for Content {
    type Error = anyhow::Error;
    fn try_from(value: forge_app::domain::ToolResult) -> std::result::Result<Self, Self::Error> {
        let call_id = value.call_id.as_ref().ok_or(Error::ToolCallMissingId)?;
        Ok(Content::ToolResult {
            tool_use_id: call_id.as_str().to_string(),
            cache_control: None,
            content: value
                .output
                .values
                .iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .next(),
            is_error: Some(value.is_error()),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum CacheControl {
    Ephemeral,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ToolChoice {
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
}

// To understand the mappings refer: https://docs.anthropic.com/en/docs/build-with-claude/tool-use#controlling-claudes-output
impl From<forge_app::domain::ToolChoice> for ToolChoice {
    fn from(value: forge_app::domain::ToolChoice) -> Self {
        match value {
            forge_app::domain::ToolChoice::Auto => {
                ToolChoice::Auto { disable_parallel_tool_use: None }
            }
            forge_app::domain::ToolChoice::Call(tool_name) => {
                ToolChoice::Tool { name: tool_name.to_string(), disable_parallel_tool_use: None }
            }
            forge_app::domain::ToolChoice::Required => {
                ToolChoice::Any { disable_parallel_tool_use: None }
            }
            forge_app::domain::ToolChoice::None => {
                ToolChoice::Auto { disable_parallel_tool_use: None }
            }
        }
    }
}

#[derive(Serialize)]
pub struct ToolDefinition {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
    input_schema: serde_json::Value,
}

impl TryFrom<forge_app::domain::ToolDefinition> for ToolDefinition {
    type Error = anyhow::Error;
    fn try_from(
        value: forge_app::domain::ToolDefinition,
    ) -> std::result::Result<Self, Self::Error> {
        Ok(ToolDefinition {
            name: value.name.to_string(),
            description: Some(value.description),
            cache_control: None,
            input_schema: serde_json::to_value(value.input_schema)?,
        })
    }
}
