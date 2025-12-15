use anyhow::{Context as _, Result};
use aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamInput;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConversationRole, InferenceConfiguration, Message, SystemContentBlock,
    ToolConfiguration,
};
use derive_more::{AsRef, Deref, From};
use forge_domain::Context;

/// Converts serde_json::Value to aws_smithy_types::Document
pub fn json_to_document(value: serde_json::Value) -> aws_smithy_types::Document {
    use std::collections::HashMap;

    use aws_smithy_types::{Document, Number};

    match value {
        serde_json::Value::Null => Document::Null,
        serde_json::Value::Bool(b) => Document::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Document::Number(Number::PosInt(i as u64))
            } else if let Some(f) = n.as_f64() {
                Document::Number(Number::Float(f))
            } else {
                Document::Null
            }
        }
        serde_json::Value::String(s) => Document::String(s),
        serde_json::Value::Array(arr) => {
            Document::Array(arr.into_iter().map(json_to_document).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: HashMap<String, Document> = obj
                .into_iter()
                .map(|(k, v)| (k, json_to_document(v)))
                .collect();
            Document::Object(map)
        }
    }
}

#[derive(Debug, AsRef, Deref, From)]
pub struct BedrockConvert(ConverseStreamInput);

impl std::ops::DerefMut for BedrockConvert {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl TryFrom<Context> for BedrockConvert {
    type Error = anyhow::Error;

    fn try_from(context: Context) -> Result<Self> {
        // Convert system messages
        let system: Vec<SystemContentBlock> = context
            .messages
            .iter()
            .filter_map(|msg| match &msg.message {
                forge_domain::ContextMessage::Text(text_msg)
                    if text_msg.has_role(forge_domain::Role::System) =>
                {
                    Some(SystemContentBlock::Text(text_msg.content.clone()))
                }
                _ => None,
            })
            .collect();

        // Convert user and assistant messages
        let messages: Vec<Message> = context
            .messages
            .into_iter()
            .filter(|message| !message.has_role(forge_domain::Role::System))
            .map(|msg| {
                convert_message(msg.message)
                    .with_context(|| "Failed to convert message to Bedrock format")
            })
            .collect::<Result<Vec<_>>>()?;

        // Convert tool configuration
        let tool_config = if !context.tools.is_empty() {
            Some(convert_tool_config(
                &context.tools,
                context.tool_choice.as_ref(),
            )?)
        } else {
            None
        };

        // Convert inference configuration
        let inference_config = if context.temperature.is_some()
            || context.top_p.is_some()
            || context.top_k.is_some()
            || context.max_tokens.is_some()
        {
            Some(
                InferenceConfiguration::builder()
                    .set_temperature(context.temperature.map(|t| t.value()))
                    .set_top_p(context.top_p.map(|t| t.value()))
                    .set_max_tokens(context.max_tokens.map(|t| t as i32))
                    .build(),
            )
        } else {
            None
        };

        let builder = ConverseStreamInput::builder()
            .set_system(if system.is_empty() {
                None
            } else {
                Some(system)
            })
            .set_messages(Some(messages))
            .set_tool_config(tool_config)
            .set_inference_config(inference_config);

        Ok(builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build Bedrock ConverseStreamInput: {}", e))?
            .into())
    }
}

/// Converts a domain ContextMessage to a Bedrock Message
fn convert_message(msg: forge_domain::ContextMessage) -> Result<Message> {
    use aws_sdk_bedrockruntime::primitives::Blob;
    use aws_sdk_bedrockruntime::types::{
        ImageBlock, ImageSource, ToolResultBlock, ToolResultContentBlock, ToolResultStatus,
        ToolUseBlock,
    };

    match msg {
        forge_domain::ContextMessage::Text(text_msg) => {
            let mut content_blocks = Vec::new();

            // Add text content if not empty
            if !text_msg.content.is_empty() {
                content_blocks.push(ContentBlock::Text(text_msg.content.clone()));
            }

            // Add tool calls if present
            if let Some(tool_calls) = text_msg.tool_calls {
                for tool_call in tool_calls {
                    let args_json: serde_json::Value =
                        serde_json::from_str(&tool_call.arguments.into_string())
                            .with_context(|| "Failed to parse tool call arguments")?;
                    let tool_use = ToolUseBlock::builder()
                        .tool_use_id(
                            tool_call
                                .call_id
                                .ok_or_else(|| anyhow::anyhow!("Tool call missing ID"))?
                                .as_str(),
                        )
                        .name(tool_call.name.to_string())
                        .input(json_to_document(args_json))
                        .build()
                        .map_err(|e| anyhow::anyhow!("Failed to build tool use block: {}", e))?;

                    content_blocks.push(ContentBlock::ToolUse(tool_use));
                }
            }

            // Map role
            let role = match text_msg.role {
                forge_domain::Role::User => ConversationRole::User,
                forge_domain::Role::Assistant => ConversationRole::Assistant,
                forge_domain::Role::System => {
                    anyhow::bail!("System messages should be filtered out before conversion")
                }
            };

            Message::builder()
                .role(role)
                .set_content(Some(content_blocks))
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build message: {}", e))
        }
        forge_domain::ContextMessage::Tool(tool_result) => {
            let is_error = tool_result.is_error();
            let tool_result_block = ToolResultBlock::builder()
                .tool_use_id(
                    tool_result
                        .call_id
                        .ok_or_else(|| anyhow::anyhow!("Tool result missing call ID"))?
                        .as_str(),
                )
                .set_content(Some(vec![ToolResultContentBlock::Text(
                    tool_result
                        .output
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Tool result has no text output"))?
                        .to_string(),
                )]))
                .status(if is_error {
                    ToolResultStatus::Error
                } else {
                    ToolResultStatus::Success
                })
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build tool result block: {}", e))?;

            Message::builder()
                .role(ConversationRole::User)
                .content(ContentBlock::ToolResult(tool_result_block))
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build tool result message: {}", e))
        }
        forge_domain::ContextMessage::Image(img) => {
            let image_block = ImageBlock::builder()
                .source(ImageSource::Bytes(Blob::new(
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, img.data())
                        .with_context(|| "Failed to decode base64 image data")?,
                )))
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build image block: {}", e))?;

            Message::builder()
                .role(ConversationRole::User)
                .content(ContentBlock::Image(image_block))
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build image message: {}", e))
        }
    }
}

/// Converts domain tool definitions and tool choice to Bedrock
/// ToolConfiguration
fn convert_tool_config(
    tools: &[forge_domain::ToolDefinition],
    tool_choice: Option<&forge_domain::ToolChoice>,
) -> Result<ToolConfiguration> {
    use aws_sdk_bedrockruntime::types::{
        AnyToolChoice, AutoToolChoice, SpecificToolChoice, Tool, ToolChoice, ToolInputSchema,
        ToolSpecification,
    };

    // Convert tool definitions
    let tool_specs: Vec<Tool> = tools
        .iter()
        .map(|tool| {
            let schema_json = serde_json::to_value(&tool.input_schema)
                .with_context(|| "Failed to serialize tool input schema")?;
            let spec = ToolSpecification::builder()
                .name(tool.name.to_string())
                .description(tool.description.clone())
                .input_schema(ToolInputSchema::Json(json_to_document(schema_json)))
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build tool specification: {}", e))?;

            Ok(Tool::ToolSpec(spec))
        })
        .collect::<Result<Vec<_>>>()?;

    // Convert tool choice
    let choice = match tool_choice {
        Some(forge_domain::ToolChoice::Auto) | None => {
            Some(ToolChoice::Auto(AutoToolChoice::builder().build()))
        }
        Some(forge_domain::ToolChoice::Required) => {
            Some(ToolChoice::Any(AnyToolChoice::builder().build()))
        }
        Some(forge_domain::ToolChoice::Call(tool_name)) => Some(ToolChoice::Tool(
            SpecificToolChoice::builder()
                .name(tool_name.to_string())
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build tool choice: {}", e))?,
        )),
        Some(forge_domain::ToolChoice::None) => None,
    };

    ToolConfiguration::builder()
        .set_tools(Some(tool_specs))
        .set_tool_choice(choice)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build tool configuration: {}", e))
}
