use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use super::request::{Message, StreamOptions};
use super::response::{ResponseUsage, ToolCall};
use super::tool_choice::ToolChoice;
use crate::domain::{
    ChatCompletionMessage, Content, Context, FinishReason, ModelId, ToolCallFull, ToolDefinition,
};
use crate::dto::openai::error::{Error, ErrorResponse};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum ResponsesInput {
    String(String),
    Messages(Vec<Message>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Setters)]
#[setters(strip_option)]
pub struct ResponsesRequest {
    pub input: ResponsesInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponseTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TextConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextResponseFormat>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum TextResponseFormat {
    #[serde(rename = "json_schema")]
    JsonSchema { json_schema: JsonSchemaFormat },
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JsonSchemaFormat {
    pub name: String,
    pub schema: serde_json::Value,
    #[serde(default = "default_strict")]
    pub strict: bool,
}

fn default_strict() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ResponseTool {
    #[serde(rename = "function")]
    Function {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        parameters: serde_json::Value,
        #[serde(default = "default_strict")]
        strict: bool,
    },
    #[serde(rename = "web_search")]
    WebSearch,
    #[serde(rename = "file_search")]
    FileSearch,
    #[serde(rename = "code_interpreter")]
    CodeInterpreter,
}

impl Default for ResponsesRequest {
    fn default() -> Self {
        ResponsesRequest {
            input: ResponsesInput::String(String::new()),
            model: None,
            instructions: None,
            text: None,
            stop: None,
            stream: None,
            max_tokens: None,
            temperature: None,
            tools: None,
            tool_choice: None,
            previous_response_id: None,
            store: Some(false),
            metadata: None,
            seed: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            logprobs: None,
            top_logprobs: None,
            stream_options: None,
        }
    }
}

impl From<ToolDefinition> for ResponseTool {
    fn from(value: ToolDefinition) -> Self {
        ResponseTool::Function {
            name: value.name.to_string(),
            description: Some(value.description),
            parameters: {
                let mut params = serde_json::to_value(value.input_schema)
                    .unwrap_or_else(|_| serde_json::json!({}));
                // Ensure OpenAI compatibility by adding properties field if missing
                if let Some(obj) = params.as_object_mut()
                    && obj.get("type") == Some(&serde_json::Value::String("object".to_string()))
                    && !obj.contains_key("properties")
                {
                    obj.insert(
                        "properties".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }
                params
            },
            strict: true,
        }
    }
}

impl From<Context> for ResponsesRequest {
    fn from(context: Context) -> Self {
        let messages = context
            .messages
            .into_iter()
            .map(Message::from)
            .collect::<Vec<_>>();

        ResponsesRequest {
            input: ResponsesInput::Messages(messages),
            tools: {
                let tools = context
                    .tools
                    .into_iter()
                    .map(ResponseTool::from)
                    .collect::<Vec<_>>();
                if tools.is_empty() { None } else { Some(tools) }
            },
            model: None,
            instructions: None,
            text: None,
            stop: None,
            stream: None,
            max_tokens: context.max_tokens.and_then(|t| u32::try_from(t).ok()),
            temperature: context.temperature.map(|t| t.value()),
            tool_choice: context.tool_choice.map(|tc| tc.into()),
            previous_response_id: None,
            store: Some(false), // Default to false for ZDR compliance
            metadata: None,
            seed: None,
            top_p: context.top_p.map(|t| t.value()),
            frequency_penalty: None,
            presence_penalty: None,
            logprobs: None,
            top_logprobs: None,
            stream_options: Some(StreamOptions { include_usage: Some(true) }),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum ResponsesResponse {
    Success {
        id: String,
        object: String,
        created_at: i64,
        model: String,
        output: Vec<ResponseItem>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ResponseUsage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    Streaming {
        id: String,
        object: String,
        created_at: i64,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output: Option<Vec<ResponseItem>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ResponseUsage>,
    },
    Failure {
        error: ErrorResponse,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ResponseItem {
    #[serde(rename = "message")]
    Message {
        id: String,
        status: MessageStatus,
        content: Vec<ResponseMessageContent>,
        role: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        id: String,
        content: Vec<ReasoningContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<Vec<String>>,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        id: String,
        call: ToolCall,
        status: FunctionCallStatus,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        id: String,
        call_id: String,
        output: serde_json::Value,
    },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum FunctionCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ResponseMessageContent {
    #[serde(rename = "output_text")]
    OutputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Vec<serde_json::Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<serde_json::Value>>,
    },
    #[serde(rename = "tool_use")]
    ToolUse { tool_call: ToolCall },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ReasoningContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "encrypted_content")]
    EncryptedContent { content: String },
}

pub fn output_text_from_response(response: &ResponsesResponse) -> Option<String> {
    match response {
        ResponsesResponse::Success { output, .. }
        | ResponsesResponse::Streaming { output: Some(output), .. } => {
            for item in output {
                if let ResponseItem::Message { content, .. } = item {
                    for msg_content in content {
                        if let ResponseMessageContent::OutputText { text, .. } = msg_content {
                            return Some(text.clone());
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

impl TryFrom<ResponsesResponse> for ChatCompletionMessage {
    type Error = anyhow::Error;

    fn try_from(res: ResponsesResponse) -> Result<Self, Self::Error> {
        match res {
            ResponsesResponse::Success { output, usage, .. }
            | ResponsesResponse::Streaming { output: Some(output), usage, .. } => {
                let mut content_text = String::new();
                let mut reasoning_text = String::new();
                let mut tool_calls = Vec::new();
                let mut finish_reason = None;

                for item in output {
                    match item {
                        ResponseItem::Message { content, status, .. } => {
                            for msg_content in content {
                                match msg_content {
                                    ResponseMessageContent::OutputText { text, .. } => {
                                        content_text.push_str(&text);
                                    }
                                    ResponseMessageContent::ToolUse { tool_call } => {
                                        if let Some(id) = tool_call.id {
                                            tool_calls.push(ToolCallFull {
                                                call_id: Some(id),
                                                name: tool_call.function.name.ok_or(
                                                    forge_domain::Error::ToolCallMissingName,
                                                )?,
                                                arguments: serde_json::from_str(
                                                    &tool_call.function.arguments,
                                                )?,
                                            });
                                        }
                                    }
                                }
                            }
                            // Map status to finish reason
                            finish_reason = match status {
                                MessageStatus::Completed => Some(FinishReason::Stop),
                                MessageStatus::Failed => Some(FinishReason::Stop), /* Use Stop as Error variant doesn't exist */
                                _ => None,
                            };
                        }
                        ResponseItem::Reasoning { content, .. } => {
                            for reasoning_content in content {
                                if let ReasoningContent::Text { text } = reasoning_content {
                                    reasoning_text.push_str(&text);
                                }
                            }
                        }
                        ResponseItem::FunctionCall { call, .. } => {
                            if let Some(id) = call.id {
                                tool_calls.push(ToolCallFull {
                                    call_id: Some(id),
                                    name: call
                                        .function
                                        .name
                                        .ok_or(forge_domain::Error::ToolCallMissingName)?,
                                    arguments: serde_json::from_str(&call.function.arguments)?,
                                });
                            }
                        }
                        _ => {}
                    }
                }

                let mut message = ChatCompletionMessage::assistant(Content::full(content_text));

                if !reasoning_text.is_empty() {
                    message = message.reasoning(Content::full(reasoning_text));
                }

                if !tool_calls.is_empty() {
                    for tool_call in tool_calls {
                        message = message.add_tool_call(tool_call);
                    }
                }

                if let Some(reason) = finish_reason {
                    message = message.finish_reason_opt(Some(reason));
                }

                if let Some(usage) = usage {
                    message.usage = Some(usage.into());
                }

                Ok(message)
            }
            ResponsesResponse::Streaming { .. } => {
                // For streaming with no output, return empty message
                Ok(ChatCompletionMessage::assistant(Content::full("")))
            }
            ResponsesResponse::Failure { error } => Err(Error::Response(error).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_responses_request_from_context() {
        use crate::domain::{Context, ContextMessage, Role, TextMessage};

        let context = Context::default().add_message(ContextMessage::Text(TextMessage {
            role: Role::User,
            content: "Hello".to_string(),
            tool_calls: None,
            model: ModelId::new("gpt-5").into(),
            reasoning_details: None,
        }));

        let request = ResponsesRequest::from(context);

        if let ResponsesInput::Messages(messages) = request.input {
            assert_eq!(messages.len(), 1);
        } else {
            panic!("Expected Messages variant");
        }
        assert_eq!(request.store, Some(false));
    }

    #[test]
    fn test_response_tool_from_tool_definition() {
        let tool_def = ToolDefinition::new("test_tool").description("A test tool");

        let response_tool = ResponseTool::from(tool_def);

        match response_tool {
            ResponseTool::Function { name, description, strict, .. } => {
                assert_eq!(name, "test_tool");
                assert_eq!(description, Some("A test tool".to_string()));
                assert_eq!(strict, true);
            }
            _ => panic!("Expected Function variant"),
        }
    }
}
