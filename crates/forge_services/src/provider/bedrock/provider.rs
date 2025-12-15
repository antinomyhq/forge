use crate::IntoDomain;
use anyhow::{Context as _, Result};
use aws_sdk_bedrockruntime::Client;
use forge_app::HttpClientService;
use forge_app::dto::bedrock::{BedrockConvert, SetCache};
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, Provider, ResultStream, Transformer,
};
use reqwest::Url;
use tokio::sync::OnceCell;

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

        // Convert context to AWS SDK types
        let bedrock_input = BedrockConvert::try_from(context)
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
            // Ignore other events
            _ => ChatCompletionMessage::assistant(Content::part("")),
        }
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
