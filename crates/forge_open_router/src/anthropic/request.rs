use derive_setters::Setters;
use forge_domain::ContextMessage;
use regex::Regex;
use serde::{Deserialize, Serialize};

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
}

impl TryFrom<forge_domain::Context> for Request {
    type Error = anyhow::Error;
    fn try_from(request: forge_domain::Context) -> std::result::Result<Self, Self::Error> {
        // note: Anthropic only supports 1 system message in context, so from the
        // context we pick the first system message available.
        // ref: https://docs.anthropic.com/en/api/messages#body-system
        let system = request.messages.iter().find_map(|message| {
            if let ContextMessage::ContentMessage(chat_message) = message {
                if chat_message.role == forge_domain::Role::System {
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
                    if let ContextMessage::ContentMessage(chat_message) = message {
                        chat_message.role != forge_domain::Role::System
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
            tool_choice: request.tool_choice.map(ToolChoice::from),
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
            ContextMessage::ContentMessage(chat_message) => {
                let mut content = Vec::with_capacity(
                    chat_message
                        .tool_calls
                        .as_ref()
                        .map(|tc| tc.len())
                        .unwrap_or_default()
                        + 1,
                );

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
                    forge_domain::Role::User => Message { role: Role::User, content },
                    forge_domain::Role::Assistant => Message { role: Role::Assistant, content },
                    forge_domain::Role::System => {
                        // note: Anthropic doesn't support system role messages and they're already
                        // filtered out. so this state is unreachable.
                        return Err(anyhow::anyhow!("system role messages are not supported in the context for anthropic provider".to_string()));
                    }
                }
            }
            ContextMessage::ToolMessage(tool_result) => {
                Message { role: Role::User, content: vec![tool_result.try_into()?] }
            }
            ContextMessage::Image(url) => {
                Message { content: vec![Content::from(url)], role: Role::User }
            }
        })
    }
}

impl From<String> for Content {
    fn from(value: String) -> Self {
        match extract_image_and_base64(&value) {
            Some((media_type, data)) => Content::Image {
                source: ImageSource {
                    type_: "base64".to_string(),
                    media_type: Some(format!("image/{}", media_type)),
                    data: Some(data),
                    url: None,
                },
            },
            None => Content::Image {
                source: ImageSource {
                    type_: "url".to_string(),
                    media_type: None,
                    data: None,
                    url: Some(value),
                },
            },
        }
    }
}

fn extract_image_and_base64(data_uri: &str) -> Option<(String, String)> {
    // Regular expression to match the data URI pattern
    let re = Regex::new(r"^data:image/(jpeg|png|webp);base64,([A-Za-z0-9+/=]+)$").unwrap();

    // Match the data URI against the regular expression
    if let Some(captures) = re.captures(data_uri) {
        // Extract image type and base64 part
        let image_type = captures.get(1).map_or("", |m| m.as_str()).to_string();
        let base64_data = captures.get(2).map_or("", |m| m.as_str()).to_string();
        Some((image_type, base64_data))
    } else {
        None
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
}

impl TryFrom<forge_domain::ToolCallFull> for Content {
    type Error = anyhow::Error;
    fn try_from(value: forge_domain::ToolCallFull) -> std::result::Result<Self, Self::Error> {
        let call_id = value
            .call_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("`call_id` is required for tool_call"))?;

        Ok(Content::ToolUse {
            id: call_id.as_str().to_string(),
            input: serde_json::to_value(value.arguments).ok(),
            name: value.name.as_str().to_string(),
            cache_control: None,
        })
    }
}

impl TryFrom<forge_domain::ToolResult> for Content {
    type Error = anyhow::Error;
    fn try_from(value: forge_domain::ToolResult) -> std::result::Result<Self, Self::Error> {
        let call_id = value
            .call_id
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("`call_id` is required for tool_result"))?;
        Ok(Content::ToolResult {
            tool_use_id: call_id.as_str().to_string(),
            cache_control: None,
            content: Some(value.content),
            is_error: Some(value.is_error),
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
impl From<forge_domain::ToolChoice> for ToolChoice {
    fn from(value: forge_domain::ToolChoice) -> Self {
        match value {
            forge_domain::ToolChoice::Auto => ToolChoice::Auto { disable_parallel_tool_use: None },
            forge_domain::ToolChoice::Call(tool_name) => ToolChoice::Tool {
                name: tool_name.into_string(),
                disable_parallel_tool_use: None,
            },
            forge_domain::ToolChoice::Required => {
                ToolChoice::Any { disable_parallel_tool_use: None }
            }
            forge_domain::ToolChoice::None => ToolChoice::Auto { disable_parallel_tool_use: None },
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

impl TryFrom<forge_domain::ToolDefinition> for ToolDefinition {
    type Error = anyhow::Error;
    fn try_from(value: forge_domain::ToolDefinition) -> std::result::Result<Self, Self::Error> {
        Ok(ToolDefinition {
            name: value.name.into_string(),
            description: Some(value.description),
            cache_control: None,
            input_schema: serde_json::to_value(value.input_schema)?,
        })
    }
}
