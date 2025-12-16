use anyhow::{Context as _, Result};
use aws_sdk_bedrockruntime::Client;
use forge_app::HttpClientService;
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ResultStream, Transformer,
};
use reqwest::Url;
use tokio::sync::OnceCell;

use super::SetCache;
use crate::{FromDomain, IntoDomain};

/// Provider implementation for Amazon Bedrock using AWS SDK
pub struct BedrockProvider<T> {
    provider: Provider<Url>,
    region: String,
    client: OnceCell<Client>,
    _phantom: std::marker::PhantomData<T>,
}

impl<H: HttpClientService> BedrockProvider<H> {
    /// Creates a new BedrockProvider instance
    ///
    /// Credentials are automatically loaded from:
    /// - Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY,
    ///   AWS_SESSION_TOKEN)
    /// - AWS credentials file (~/.aws/credentials)
    /// - IAM role (for EC2/ECS/Lambda)
    pub fn new(provider: Provider<Url>) -> Result<Self> {
        // Extract region from URL params
        let region_param: forge_domain::URLParam = "AWS_REGION".to_string().into();
        let region = provider
            .credential
            .as_ref()
            .and_then(|c| c.url_params.get(&region_param).map(|v| v.to_string()))
            .unwrap_or_else(|| "us-east-1".to_string());

        Ok(Self {
            provider,
            region,
            client: OnceCell::new(),
            _phantom: std::marker::PhantomData,
        })
    }

    /// Get or create the AWS SDK client
    async fn get_client(&self) -> Result<&Client> {
        self.client
            .get_or_try_init(|| async {
                let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(aws_config::Region::new(self.region.clone()))
                    .load()
                    .await;
                Ok(Client::new(&config))
            })
            .await
    }

    /// Check if the model supports prompt caching
    ///
    /// AWS Bedrock supports prompt caching for models that implement cache
    /// points. Currently supported models:
    /// - Anthropic Claude (all variants) - System + Message cache points
    /// - Amazon Nova (all variants) - System cache points only (20K token
    ///   limit)
    ///
    /// The SetCache transformer is model-aware and will only add message-level
    /// cache points for Claude models.
    fn supports_caching(model_id: &str) -> bool {
        let model_lower = model_id.to_lowercase();

        // Claude and Nova models support prompt caching
        // SetCache is model-aware: adds message cache points only for Claude
        model_lower.contains("anthropic") || model_lower.contains("claude")
    }

    /// Transform model ID with regional prefix if needed
    pub fn transform_model_id(&self, model_id: &str) -> String {
        // Skip if already has global prefix
        if model_id.starts_with("global.") {
            return model_id.to_string();
        }

        // Determine regional prefix
        let prefix = match self.region.as_str() {
            r if r.starts_with("us-") && !r.contains("gov") => "us.",
            r if r.starts_with("eu-") => "eu.",
            "ap-southeast-2" => "au.",
            r if r.starts_with("ap-") => "apac.",
            _ => "",
        };

        // Only prefix Anthropic models that don't already have a regional prefix
        if model_id.contains("anthropic.")
            && !model_id.starts_with("us.")
            && !model_id.starts_with("eu.")
            && !model_id.starts_with("apac.")
            && !model_id.starts_with("au.")
        {
            format!("{}{}", prefix, model_id)
        } else {
            model_id.to_string()
        }
    }

    /// Perform a streaming chat completion
    pub async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let model_id = self.transform_model_id(model.as_str());
        let client = match self.get_client().await {
            Ok(c) => c,
            Err(e) => return Err(e),
        };

        // Convert context to AWS SDK types using FromDomain trait
        let bedrock_input =
            aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamInput::from_domain(
                context,
            )
            .context("Failed to convert context to Bedrock ConverseStreamInput")?;

        // Apply transformers pipeline
        let supports_caching = Self::supports_caching(&model_id);
        let bedrock_input = SetCache
            .when(move |_| supports_caching)
            .transform(bedrock_input);

        // Build and send the converse_stream request
        let output = client
            .converse_stream()
            .model_id(model_id)
            .set_system(bedrock_input.system.clone())
            .set_messages(bedrock_input.messages.clone())
            .set_tool_config(bedrock_input.tool_config.clone())
            .set_inference_config(bedrock_input.inference_config.clone())
            .send()
            .await
            .context("Failed to call Bedrock converse_stream API")?;

        // Convert the Bedrock event stream to ChatCompletionMessage stream
        let stream = futures::stream::unfold(output.stream, |mut event_stream| async move {
            match event_stream.recv().await {
                Ok(Some(event)) => {
                    let message = event.into_domain();
                    Some((Ok(message), event_stream))
                }
                Ok(None) => None, // End of stream
                Err(e) => Some((
                    Err(anyhow::anyhow!("Bedrock stream error: {:?}", e)),
                    event_stream,
                )),
            }
        });

        Ok(Box::pin(stream))
    }

    /// Get available models
    pub async fn models(&self) -> Result<Vec<Model>> {
        // Bedrock doesn't have a models list API
        // Return hardcoded models from configuration
        match &self.provider.models {
            Some(forge_domain::ModelSource::Hardcoded(models)) => Ok(models.clone()),
            _ => Ok(vec![]),
        }
    }
}

/// Converts Bedrock stream events to ChatCompletionMessage
impl IntoDomain for aws_sdk_bedrockruntime::types::ConverseStreamOutput {
    type Domain = forge_domain::ChatCompletionMessage;

    fn into_domain(self) -> Self::Domain {
        use aws_sdk_bedrockruntime::types::ConverseStreamOutput;
        use forge_domain::{
            ChatCompletionMessage, Content, FinishReason, ToolCallId, ToolCallPart, ToolName,
        };

        match self {
            ConverseStreamOutput::ContentBlockDelta(delta) => {
                if let Some(delta_content) = delta.delta {
                    match delta_content {
                        aws_sdk_bedrockruntime::types::ContentBlockDelta::Text(text) => {
                            ChatCompletionMessage::assistant(Content::part(text))
                        }
                        aws_sdk_bedrockruntime::types::ContentBlockDelta::ToolUse(tool_use) => {
                            // Tool use delta - partial JSON for tool arguments
                            ChatCompletionMessage::assistant(Content::part("")).add_tool_call(
                                ToolCallPart {
                                    call_id: None,
                                    name: None,
                                    arguments_part: tool_use.input,
                                },
                            )
                        }
                        aws_sdk_bedrockruntime::types::ContentBlockDelta::ReasoningContent(
                            reasoning,
                        ) => {
                            // Handle reasoning content delta
                            match reasoning {
                                aws_sdk_bedrockruntime::types::ReasoningContentBlockDelta::Text(
                                    text,
                                ) => {
                                    // Reasoning text - add to both reasoning field and as detail part
                                    ChatCompletionMessage::default()
                                        .reasoning(Content::part(text.clone()))
                                        .add_reasoning_detail(forge_domain::Reasoning::Part(vec![
                                            forge_domain::ReasoningPart {
                                                text: Some(text),
                                                signature: None,
                                                ..Default::default()
                                            },
                                        ]))
                                }
                                aws_sdk_bedrockruntime::types::ReasoningContentBlockDelta::Signature(
                                    sig,
                                ) => {
                                    // Signature for reasoning - add as reasoning detail part
                                    ChatCompletionMessage::default().add_reasoning_detail(
                                        forge_domain::Reasoning::Part(vec![
                                            forge_domain::ReasoningPart {
                                                text: None,
                                                signature: Some(sig),
                                                ..Default::default()
                                            },
                                        ]),
                                    )
                                }
                                aws_sdk_bedrockruntime::types::ReasoningContentBlockDelta::RedactedContent(_) => {
                                    // Redacted content - skip it
                                    ChatCompletionMessage::default()
                                }
                                _ => ChatCompletionMessage::default(),
                            }
                        }
                        _ => ChatCompletionMessage::assistant(Content::part("")),
                    }
                } else {
                    ChatCompletionMessage::assistant(Content::part(""))
                }
            }
            ConverseStreamOutput::ContentBlockStart(start) => {
                if let Some(start_content) = start.start {
                    match start_content {
                        aws_sdk_bedrockruntime::types::ContentBlockStart::ToolUse(tool_use) => {
                            // Tool use start - contains tool name and ID
                            ChatCompletionMessage::assistant(Content::part("")).add_tool_call(
                                ToolCallPart {
                                    call_id: Some(ToolCallId::new(tool_use.tool_use_id)),
                                    name: Some(ToolName::new(tool_use.name)),
                                    arguments_part: String::new(),
                                },
                            )
                        }
                        _ => ChatCompletionMessage::assistant(Content::part("")),
                    }
                } else {
                    ChatCompletionMessage::assistant(Content::part(""))
                }
            }
            ConverseStreamOutput::MessageStop(stop) => {
                // Message stop contains finish reason
                let finish_reason = match &stop.stop_reason {
                    aws_sdk_bedrockruntime::types::StopReason::EndTurn => FinishReason::Stop,
                    aws_sdk_bedrockruntime::types::StopReason::MaxTokens => FinishReason::Length,
                    aws_sdk_bedrockruntime::types::StopReason::ToolUse => FinishReason::ToolCalls,
                    aws_sdk_bedrockruntime::types::StopReason::ContentFiltered => {
                        FinishReason::ContentFilter
                    }
                    _ => FinishReason::Stop,
                };

                ChatCompletionMessage::assistant(Content::part(""))
                    .finish_reason_opt(Some(finish_reason))
            }
            ConverseStreamOutput::Metadata(metadata) => {
                // Metadata contains usage information
                let usage = metadata.usage.map(|u| {
                    // AWS Bedrock supports cache tokens but not reasoning tokens
                    // Sum both cache read and cache write tokens into cached_tokens field
                    let cached_tokens = u
                        .cache_read_input_tokens
                        .unwrap_or(0)
                        .saturating_add(u.cache_write_input_tokens.unwrap_or(0));

                    forge_domain::Usage {
                        prompt_tokens: forge_domain::TokenCount::Actual(u.total_tokens as usize),
                        completion_tokens: forge_domain::TokenCount::Actual(
                            u.output_tokens as usize,
                        ),
                        total_tokens: forge_domain::TokenCount::Actual(u.total_tokens as usize),
                        cached_tokens: forge_domain::TokenCount::Actual(cached_tokens as usize),
                        ..Default::default()
                    }
                });

                let mut msg = ChatCompletionMessage::assistant(Content::part(""));
                if let Some(u) = usage {
                    msg = msg.usage(u);
                }
                msg
            }
            ConverseStreamOutput::ContentBlockStop(_) => ChatCompletionMessage::assistant("").finish_reason(FinishReason::Stop),
            _ => ChatCompletionMessage::assistant(Content::part("")),
        }
    }
}

/// Converts domain Context to Bedrock ConverseStreamInput
impl FromDomain<forge_domain::Context>
    for aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamInput
{
    fn from_domain(context: forge_domain::Context) -> anyhow::Result<Self> {
        use aws_sdk_bedrockruntime::operation::converse_stream::ConverseStreamInput;
        use aws_sdk_bedrockruntime::types::{InferenceConfiguration, Message, SystemContentBlock};

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
                Message::from_domain(msg.message)
                    .with_context(|| "Failed to convert message to Bedrock format")
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        // Convert tool configuration
        let tool_config = if !context.tools.is_empty() {
            use aws_sdk_bedrockruntime::types::{Tool, ToolChoice, ToolConfiguration};

            let tool_specs: Vec<Tool> = context
                .tools
                .into_iter()
                .map(Tool::from_domain)
                .collect::<anyhow::Result<Vec<_>>>()?;

            let choice = context
                .tool_choice
                .filter(|c| !matches!(c, forge_domain::ToolChoice::None))
                .map(ToolChoice::from_domain)
                .transpose()?;

            Some(
                ToolConfiguration::builder()
                    .set_tools(Some(tool_specs))
                    .set_tool_choice(choice)
                    .build()
                    .map_err(|e| anyhow::anyhow!("Failed to build tool configuration: {}", e))?,
            )
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

        builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build Bedrock ConverseStreamInput: {}", e))
    }
}

/// Converts a domain ContextMessage to a Bedrock Message
impl FromDomain<forge_domain::ContextMessage> for aws_sdk_bedrockruntime::types::Message {
    fn from_domain(msg: forge_domain::ContextMessage) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        use aws_sdk_bedrockruntime::primitives::Blob;
        use aws_sdk_bedrockruntime::types::{
            ContentBlock, ConversationRole, ImageBlock, ImageSource, Message, ToolResultBlock,
            ToolResultContentBlock, ToolResultStatus, ToolUseBlock,
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
                        let tool_use = ToolUseBlock::builder()
                            .tool_use_id(
                                tool_call
                                    .call_id
                                    .ok_or_else(|| anyhow::anyhow!("Tool call missing ID"))?
                                    .as_str(),
                            )
                            .name(tool_call.name.to_string())
                            .input(aws_smithy_types::Document::from_domain(
                                tool_call.arguments,
                            )?)
                            .build()
                            .map_err(|e| {
                                anyhow::anyhow!("Failed to build tool use block: {}", e)
                            })?;

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
                        base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            img.data(),
                        )
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
}

/// Converts schemars RootSchema to AWS Bedrock ToolInputSchema
impl FromDomain<schemars::schema::RootSchema> for aws_sdk_bedrockruntime::types::ToolInputSchema {
    fn from_domain(schema: schemars::schema::RootSchema) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        use aws_sdk_bedrockruntime::types::ToolInputSchema;

        // Serialize RootSchema to JSON value first
        let json_value =
            serde_json::to_value(&schema).with_context(|| "Failed to serialize RootSchema")?;

        // Convert JSON value to Document and wrap in ToolInputSchema
        Ok(ToolInputSchema::Json(json_value_to_document(json_value)))
    }
}

/// Converts ToolCallArguments to AWS Smithy Document
impl FromDomain<forge_domain::ToolCallArguments> for aws_smithy_types::Document {
    fn from_domain(args: forge_domain::ToolCallArguments) -> anyhow::Result<Self> {
        use anyhow::Context as _;

        // Parse the arguments to get a serde_json::Value
        let json_value = args
            .parse()
            .with_context(|| "Failed to parse tool call arguments")?;

        // Convert JSON value to Document
        Ok(json_value_to_document(json_value))
    }
}

/// Helper function to convert serde_json::Value to aws_smithy_types::Document
fn json_value_to_document(value: serde_json::Value) -> aws_smithy_types::Document {
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
            Document::Array(arr.into_iter().map(json_value_to_document).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: HashMap<String, Document> = obj
                .into_iter()
                .map(|(k, v)| (k, json_value_to_document(v)))
                .collect();
            Document::Object(map)
        }
    }
}

/// Converts domain ToolDefinition to Bedrock Tool
impl FromDomain<forge_domain::ToolDefinition> for aws_sdk_bedrockruntime::types::Tool {
    fn from_domain(tool: forge_domain::ToolDefinition) -> anyhow::Result<Self> {
        use aws_sdk_bedrockruntime::types::{Tool, ToolInputSchema, ToolSpecification};

        let spec = ToolSpecification::builder()
            .name(tool.name.to_string())
            .description(tool.description.clone())
            .input_schema(ToolInputSchema::from_domain(tool.input_schema)?)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build tool specification: {}", e))?;

        Ok(Tool::ToolSpec(spec))
    }
}

/// Converts domain ToolChoice to Bedrock ToolChoice
impl FromDomain<forge_domain::ToolChoice> for aws_sdk_bedrockruntime::types::ToolChoice {
    fn from_domain(choice: forge_domain::ToolChoice) -> anyhow::Result<Self> {
        use aws_sdk_bedrockruntime::types::{
            AnyToolChoice, AutoToolChoice, SpecificToolChoice, ToolChoice,
        };

        let bedrock_choice = match choice {
            forge_domain::ToolChoice::Auto => ToolChoice::Auto(AutoToolChoice::builder().build()),
            forge_domain::ToolChoice::Required => ToolChoice::Any(AnyToolChoice::builder().build()),
            forge_domain::ToolChoice::Call(tool_name) => ToolChoice::Tool(
                SpecificToolChoice::builder()
                    .name(tool_name.to_string())
                    .build()
                    .map_err(|e| anyhow::anyhow!("Failed to build tool choice: {}", e))?,
            ),
            forge_domain::ToolChoice::None => {
                // For None, we'll return a default Auto choice, but the caller should handle
                // this by not setting tool_choice at all
                ToolChoice::Auto(AutoToolChoice::builder().build())
            }
        };

        Ok(bedrock_choice)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockHttpClient;

    #[async_trait::async_trait]
    impl HttpClientService for MockHttpClient {
        async fn get(
            &self,
            _url: &reqwest::Url,
            _headers: Option<reqwest::header::HeaderMap>,
        ) -> anyhow::Result<reqwest::Response> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }

        async fn post(
            &self,
            _url: &reqwest::Url,
            _body: bytes::Bytes,
        ) -> anyhow::Result<reqwest::Response> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }

        async fn delete(&self, _url: &reqwest::Url) -> anyhow::Result<reqwest::Response> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }

        async fn eventsource(
            &self,
            _url: &reqwest::Url,
            _headers: Option<reqwest::header::HeaderMap>,
            _body: bytes::Bytes,
        ) -> anyhow::Result<reqwest_eventsource::EventSource> {
            Err(anyhow::anyhow!("Mock HTTP client - no real requests"))
        }
    }

    fn create_test_provider(region: &str) -> BedrockProvider<MockHttpClient> {
        use forge_domain::{
            ApiKey, AuthCredential, AuthDetails, ProviderId, ProviderResponse, ProviderType,
        };
        use reqwest::Url;

        let provider = Provider {
            id: ProviderId::from("bedrock".to_string()),
            provider_type: ProviderType::Llm,
            response: Some(ProviderResponse::Bedrock),
            url: Url::parse("https://bedrock-runtime.us-east-1.amazonaws.com").unwrap(),
            models: None,
            auth_methods: vec![],
            url_params: vec![],
            credential: Some(AuthCredential {
                id: ProviderId::from("bedrock".to_string()),
                auth_details: AuthDetails::ApiKey(ApiKey::from("test-token".to_string())),
                url_params: std::collections::HashMap::new(),
            }),
        };

        BedrockProvider {
            provider,
            client: OnceCell::new(),
            region: region.to_string(),
            _phantom: std::marker::PhantomData,
        }
    }

    #[test]
    fn test_transform_model_id_us_region() {
        let bedrock = create_test_provider("us-east-1");
        let transformed = bedrock.transform_model_id("anthropic.claude-3-5-sonnet-20241022-v2:0");
        assert_eq!(transformed, "us.anthropic.claude-3-5-sonnet-20241022-v2:0");
    }

    #[test]
    fn test_transform_model_id_eu_region() {
        let bedrock = create_test_provider("eu-west-1");
        let transformed = bedrock.transform_model_id("anthropic.claude-3-5-sonnet-20241022-v2:0");
        assert_eq!(transformed, "eu.anthropic.claude-3-5-sonnet-20241022-v2:0");
    }

    #[test]
    fn test_transform_model_id_already_prefixed() {
        let bedrock = create_test_provider("us-east-1");
        let transformed =
            bedrock.transform_model_id("us.anthropic.claude-3-5-sonnet-20241022-v2:0");
        assert_eq!(transformed, "us.anthropic.claude-3-5-sonnet-20241022-v2:0");
    }

    #[test]
    fn test_transform_model_id_non_anthropic() {
        let bedrock = create_test_provider("us-east-1");
        let transformed = bedrock.transform_model_id("amazon.nova-pro-v1:0");
        assert_eq!(transformed, "amazon.nova-pro-v1:0");
    }
}
