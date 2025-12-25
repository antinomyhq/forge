/// OpenAI Responses API Provider for Codex models
///
/// This provider handles OpenAI's Codex models (e.g., gpt-5.1-codex,
/// codex-mini-latest) which use the Responses API instead of the standard Chat
/// Completions API.
///
/// The Responses API provides a different request/response format optimized for
/// reasoning and coding tasks, with stricter JSON schema validation for tools.
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context as _;
use async_openai::Client as AsyncOpenAIClient;
use async_openai::config::OpenAIConfig;
use async_openai::traits::RequestOptionsBuilder as _;
use async_openai::types::responses as oai;
use derive_setters::Setters;
use forge_app::HttpInfra;
use forge_app::domain::{
    ChatCompletionMessage, Content, Context as ChatContext, ContextMessage, FinishReason, Model,
    ModelId, ResultStream, RetryConfig, Role, TokenCount, ToolCall, ToolCallArguments,
    ToolCallFull, ToolCallId, ToolCallPart, ToolChoice, ToolName, Usage,
};
use forge_domain::{ChatRepository, Provider};
use futures::StreamExt;
use reqwest::header::AUTHORIZATION;
use tracing::info;
use url::Url;
use crate::provider::utils::{create_headers, format_http_context, sanitize_headers};
use crate::provider::{FromDomain, IntoDomain};
use crate::provider::retry::into_retry;

#[derive(Clone)]
struct OpenAIResponsesProvider<H> {
    provider: Provider<Url>,
    client: Arc<AsyncOpenAIClient<OpenAIConfig>>,
    api_base: Url,
    responses_url: Url,
    _phantom: std::marker::PhantomData<H>,
}

impl<H: HttpInfra> OpenAIResponsesProvider<H> {
    /// Creates a new OpenAI Responses provider
    ///
    /// # Panics
    ///
    /// Panics if the provider URL cannot be converted to an API base URL
    pub fn new(provider: Provider<Url>) -> Self {
        let api_base = api_base_from_endpoint_url(&provider.url)
            .expect("Failed to derive API base URL from provider endpoint");
        let responses_url = responses_endpoint_from_api_base(&api_base);

        let api_key = provider
            .credential
            .as_ref()
            .map(|c| match &c.auth_details {
                forge_domain::AuthDetails::ApiKey(key) => key.as_str(),
                forge_domain::AuthDetails::OAuthWithApiKey { api_key, .. } => api_key.as_str(),
                forge_domain::AuthDetails::OAuth { tokens, .. } => tokens.access_token.as_str(),
            })
            .unwrap_or("");

        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(api_base.as_str());

        let client = Arc::new(AsyncOpenAIClient::with_config(config));

        Self {
            provider,
            client,
            api_base,
            responses_url,
            _phantom: std::marker::PhantomData,
        }
    }

    fn get_headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if let Some(api_key) = self
            .provider
            .credential
            .as_ref()
            .map(|c| match &c.auth_details {
                forge_domain::AuthDetails::ApiKey(key) => key.as_str(),
                forge_domain::AuthDetails::OAuthWithApiKey { api_key, .. } => api_key.as_str(),
                forge_domain::AuthDetails::OAuth { tokens, .. } => tokens.access_token.as_str(),
            })
        {
            headers.push((AUTHORIZATION.to_string(), format!("Bearer {api_key}")));
        }
        self.provider
            .auth_methods
            .iter()
            .for_each(|method| match method {
                forge_domain::AuthMethod::ApiKey => {}
                forge_domain::AuthMethod::OAuthDevice(oauth_config) => {
                    if let Some(custom_headers) = &oauth_config.custom_headers {
                        custom_headers.iter().for_each(|(k, v)| {
                            headers.push((k.clone(), v.clone()));
                        });
                    }
                }
                forge_domain::AuthMethod::OAuthCode(oauth_config) => {
                    if let Some(custom_headers) = &oauth_config.custom_headers {
                        custom_headers.iter().for_each(|(k, v)| {
                            headers.push((k.clone(), v.clone()));
                        });
                    }
                }
            });
        headers
    }
}

impl<T: HttpInfra> OpenAIResponsesProvider<T> {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let headers = create_headers(self.get_headers());

        let stream_requested = context.stream.unwrap_or(true);
        let mut request = oai::CreateResponse::from_domain(context)?;
        request.model = Some(model.as_str().to_string());

        info!(
            url = %self.responses_url,
            base_url = %self.api_base,
            model = %model,
            headers = ?sanitize_headers(&headers),
            message_count = %request_message_count(&request),
            stream = %stream_requested,
            "Connecting Upstream (Codex via Responses API)"
        );

        if stream_requested {
            let stream = self
                .client
                .responses()
                .headers(headers)
                .create_stream(request)
                .await
                .with_context(|| format_http_context(None, "POST", &self.responses_url))?;

            let stream = into_chat_completion_message_codex(self.responses_url.clone(), stream);

            Ok(Box::pin(stream))
        } else {
            let response = self
                .client
                .responses()
                .headers(headers)
                .create(request)
                .await
                .with_context(|| format_http_context(None, "POST", &self.responses_url))?;

            let message = response.into_domain();
            let stream = tokio_stream::iter([Ok(message)]);
            Ok(Box::pin(stream))
        }
    }
}

/// Derives an API base URL suitable for `async-openai` from a configured
/// endpoint URL.
///
/// For Codex/Responses usage we only need the host and the `/v1` prefix.
/// Any path on the incoming endpoint is ignored in favor of `/v1`.
fn api_base_from_endpoint_url(endpoint: &Url) -> anyhow::Result<Url> {
    let mut base = endpoint.clone();
    base.set_path("/v1");
    base.set_query(None);
    base.set_fragment(None);
    Ok(base)
}

fn responses_endpoint_from_api_base(api_base: &Url) -> Url {
    let mut url = api_base.clone();

    let mut path = api_base.path().trim_end_matches('/').to_string();
    path.push_str("/responses");

    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);

    url
}

fn request_message_count(request: &oai::CreateResponse) -> usize {
    match &request.input {
        oai::InputParam::Text(_) => 1,
        oai::InputParam::Items(items) => items.len(),
    }
}

impl FromDomain<ToolChoice> for oai::ToolChoiceParam {
    fn from_domain(choice: ToolChoice) -> anyhow::Result<Self> {
        Ok(match choice {
            ToolChoice::None => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::None),
            ToolChoice::Auto => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Auto),
            ToolChoice::Required => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Required),
            ToolChoice::Call(name) => {
                oai::ToolChoiceParam::Function(oai::ToolChoiceFunction { name: name.to_string() })
            }
        })
    }
}

fn normalize_openai_json_schema(schema: &mut serde_json::Value) {
    match schema {
        serde_json::Value::Object(map) => {
            let is_object = map
                .get("type")
                .and_then(|value| value.as_str())
                .is_some_and(|ty| ty == "object")
                || map.contains_key("properties");

            if is_object {
                if !map.contains_key("properties") {
                    map.insert(
                        "properties".to_string(),
                        serde_json::Value::Object(serde_json::Map::new()),
                    );
                }

                // OpenAI requires this field to exist and be `false` for objects.
                map.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );

                // OpenAI requires `required` to exist and include every property key.
                let required_keys = map
                    .get("properties")
                    .and_then(|value| value.as_object())
                    .map(|props| {
                        let mut keys = props.keys().cloned().collect::<Vec<_>>();
                        keys.sort();
                        keys
                    })
                    .unwrap_or_default();

                let required_values = required_keys
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect::<Vec<_>>();

                map.insert(
                    "required".to_string(),
                    serde_json::Value::Array(required_values),
                );
            }

            for value in map.values_mut() {
                normalize_openai_json_schema(value);
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                normalize_openai_json_schema(value);
            }
        }
        _ => {}
    }
}

fn codex_tool_parameters(
    schema: &schemars::schema::RootSchema,
) -> anyhow::Result<serde_json::Value> {
    let mut params =
        serde_json::to_value(schema).with_context(|| "Failed to serialize tool schema")?;

    // The Responses API performs strict JSON Schema validation for tools; normalize
    // schemars output into the subset OpenAI accepts.
    normalize_openai_json_schema(&mut params);

    Ok(params)
}

/// Converts Forge's domain-level Context into an async-openai Responses API
/// request.
///
/// Supported subset (first iteration):
/// - Text messages (system/user/assistant)
/// - Assistant tool calls (full)
/// - Tool results
/// - tools + tool_choice
/// - max_tokens, temperature, top_p
impl FromDomain<ChatContext> for oai::CreateResponse {
    fn from_domain(context: ChatContext) -> anyhow::Result<Self> {
        let mut instructions: Vec<String> = Vec::new();
        let mut items: Vec<oai::InputItem> = Vec::new();

        for entry in context.messages {
            match entry.message {
                ContextMessage::Text(message) => match message.role {
                    Role::System => {
                        instructions.push(message.content);
                    }
                    Role::User => {
                        items.push(oai::InputItem::EasyMessage(oai::EasyInputMessage {
                            r#type: oai::MessageType::Message,
                            role: oai::Role::User,
                            content: oai::EasyInputContent::Text(message.content),
                        }));
                    }
                    Role::Assistant => {
                        if !message.content.trim().is_empty() {
                            items.push(oai::InputItem::EasyMessage(oai::EasyInputMessage {
                                r#type: oai::MessageType::Message,
                                role: oai::Role::Assistant,
                                content: oai::EasyInputContent::Text(message.content),
                            }));
                        }

                        if let Some(tool_calls) = message.tool_calls {
                            for call in tool_calls {
                                let call_id = call
                                    .call_id
                                    .as_ref()
                                    .map(|id| id.as_str().to_string())
                                    .ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "Tool call is missing call_id; cannot be sent to Responses API"
                                    )
                                })?;

                                items.push(oai::InputItem::Item(oai::Item::FunctionCall(
                                    oai::FunctionToolCall {
                                        arguments: call.arguments.into_string(),
                                        call_id,
                                        name: call.name.to_string(),
                                        id: None,
                                        status: None,
                                    },
                                )));
                            }
                        }
                    }
                },
                ContextMessage::Tool(result) => {
                    let call_id = result
                        .call_id
                        .as_ref()
                        .map(|id| id.as_str().to_string())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Tool result is missing call_id; cannot be sent to Responses API"
                            )
                        })?;

                    let output_json = serde_json::to_string(&result.output)
                        .with_context(|| "Failed to serialize tool output as JSON")?;

                    items.push(oai::InputItem::Item(oai::Item::FunctionCallOutput(
                        oai::FunctionCallOutputItemParam {
                            call_id,
                            output: oai::FunctionCallOutput::Text(output_json),
                            id: None,
                            status: None,
                        },
                    )));
                }
                ContextMessage::Image(_) => {
                    anyhow::bail!("Codex (Responses API) path does not yet support image inputs");
                }
            }
        }

        let instructions = (!instructions.is_empty()).then(|| instructions.join("\n\n"));

        let max_output_tokens = context
            .max_tokens
            .map(|tokens| u32::try_from(tokens).context("max_tokens must fit into u32"))
            .transpose()?;

        let tools = (!context.tools.is_empty())
            .then(|| {
                context
                    .tools
                    .into_iter()
                    .map(|tool| {
                        Ok(oai::Tool::Function(oai::FunctionTool {
                            name: tool.name.to_string(),
                            parameters: Some(codex_tool_parameters(&tool.input_schema)?),
                            strict: Some(true),
                            description: Some(tool.description),
                        }))
                    })
                    .collect::<anyhow::Result<Vec<oai::Tool>>>()
            })
            .transpose()?;

        let tool_choice = context
            .tool_choice
            .map(oai::ToolChoiceParam::from_domain)
            .transpose()?;

        let mut builder = oai::CreateResponseArgs::default();
        builder.input(oai::InputParam::Items(items));

        if let Some(instructions) = instructions {
            builder.instructions(instructions);
        }

        if let Some(max_output_tokens) = max_output_tokens {
            builder.max_output_tokens(max_output_tokens);
        }

        if let Some(temperature) = context.temperature.map(|t| t.value()) {
            builder.temperature(temperature);
        }

        // Some OpenAI Codex/"reasoning" models reject `top_p` entirely (even when set
        // to defaults). To avoid hard failures, we currently omit it for the
        // Responses API path.

        if let Some(tools) = tools {
            builder.tools(tools);
        }

        if let Some(tool_choice) = tool_choice {
            builder.tool_choice(tool_choice);
        }

        // Enable reasoning for o-series and gpt-5 models
        // This is required to receive reasoning text in the response
        let reasoning_config = oai::ReasoningArgs::default()
            .effort(oai::ReasoningEffort::Medium)
            .summary(oai::ReasoningSummary::Auto)
            .build()
            .map_err(anyhow::Error::from)?;
        builder.reasoning(reasoning_config);

        builder.build().map_err(anyhow::Error::from)
    }
}

impl IntoDomain for oai::ResponseUsage {
    type Domain = Usage;

    fn into_domain(self) -> Self::Domain {
        Usage {
            prompt_tokens: TokenCount::Actual(self.input_tokens as usize),
            completion_tokens: TokenCount::Actual(self.output_tokens as usize),
            total_tokens: TokenCount::Actual(self.total_tokens as usize),
            cached_tokens: TokenCount::Actual(self.input_tokens_details.cached_tokens as usize),
            cost: None,
        }
    }
}

impl IntoDomain for oai::Response {
    type Domain = ChatCompletionMessage;

    fn into_domain(self) -> Self::Domain {
        let mut message = ChatCompletionMessage::default();

        if let Some(text) = self.output_text() {
            message = message.content_full(text);
        }

        let mut saw_tool_call = false;
        for item in &self.output {
            match item {
                oai::OutputItem::FunctionCall(call) => {
                    saw_tool_call = true;
                    message = message.add_tool_call(ToolCall::Full(ToolCallFull {
                        call_id: Some(ToolCallId::new(call.call_id.clone())),
                        name: ToolName::new(call.name.clone()),
                        arguments: ToolCallArguments::from_json(&call.arguments),
                    }));
                }
                oai::OutputItem::Reasoning(reasoning) => {
                    let mut all_reasoning_text = String::new();

                    // Process reasoning text content
                    if let Some(content) = &reasoning.content {
                        let reasoning_text =
                            content.iter().map(|c| c.text.as_str()).collect::<String>();
                        if !reasoning_text.is_empty() {
                            all_reasoning_text.push_str(&reasoning_text);
                            message =
                                message.add_reasoning_detail(forge_domain::Reasoning::Full(vec![
                                    forge_domain::ReasoningFull {
                                        text: Some(reasoning_text),
                                        type_of: Some("reasoning.text".to_string()),
                                        ..Default::default()
                                    },
                                ]));
                        }
                    }

                    // Process reasoning summary
                    if !reasoning.summary.is_empty() {
                        let mut summary_texts = Vec::new();
                        for summary_part in &reasoning.summary {
                            match summary_part {
                                oai::SummaryPart::SummaryText(summary) => {
                                    summary_texts.push(summary.text.clone());
                                }
                            }
                        }
                        let summary_text = summary_texts.join("");
                        if !summary_text.is_empty() {
                            all_reasoning_text.push_str(&summary_text);
                            message =
                                message.add_reasoning_detail(forge_domain::Reasoning::Full(vec![
                                    forge_domain::ReasoningFull {
                                        text: Some(summary_text),
                                        type_of: Some("reasoning.summary".to_string()),
                                        ..Default::default()
                                    },
                                ]));
                        }
                    }

                    // Set the combined reasoning text in the reasoning field
                    if !all_reasoning_text.is_empty() {
                        message = message.reasoning(Content::full(all_reasoning_text));
                    }
                }
                _ => {}
            }
        }

        if let Some(usage) = self.usage {
            message = message.usage(usage.into_domain());
        }

        message = message.finish_reason_opt(Some(if saw_tool_call {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        }));

        message
    }
}

#[derive(Default)]
struct CodexStreamState {
    output_index_to_tool_call: HashMap<u32, (ToolCallId, ToolName)>,
}

fn into_chat_completion_message_codex(
    url: Url,
    stream: oai::ResponseStream,
) -> impl tokio_stream::Stream<Item = anyhow::Result<ChatCompletionMessage>> {
    stream
        .scan(CodexStreamState::default(), move |state, event| {
            futures::future::ready({
                let item = match event {
                    Ok(event) => match event {
                        oai::ResponseStreamEvent::ResponseOutputTextDelta(delta) => Some(Ok(
                            ChatCompletionMessage::assistant(Content::part(delta.delta)),
                        )),
                        oai::ResponseStreamEvent::ResponseReasoningTextDelta(delta) => {
                            Some(Ok(ChatCompletionMessage::default()
                                .reasoning(Content::part(delta.delta.clone()))
                                .add_reasoning_detail(forge_domain::Reasoning::Part(vec![
                                    forge_domain::ReasoningPart {
                                        text: Some(delta.delta),
                                        type_of: Some("reasoning.text".to_string()),
                                        ..Default::default()
                                    },
                                ]))))
                        }
                        oai::ResponseStreamEvent::ResponseReasoningSummaryTextDelta(delta) => {
                            Some(Ok(ChatCompletionMessage::default()
                                .reasoning(Content::part(delta.delta.clone()))
                                .add_reasoning_detail(forge_domain::Reasoning::Part(vec![
                                    forge_domain::ReasoningPart {
                                        text: Some(delta.delta),
                                        type_of: Some("reasoning.summary".to_string()),
                                        ..Default::default()
                                    },
                                ]))))
                        }
                        oai::ResponseStreamEvent::ResponseOutputItemAdded(added) => {
                            match &added.item {
                                oai::OutputItem::FunctionCall(call) => {
                                    let tool_call_id = ToolCallId::new(call.call_id.clone());
                                    let tool_name = ToolName::new(call.name.clone());

                                    state.output_index_to_tool_call.insert(
                                        added.output_index,
                                        (tool_call_id.clone(), tool_name.clone()),
                                    );

                                    // Only emit if we have non-empty initial arguments.
                                    // Otherwise, wait for deltas or done event.
                                    if !call.arguments.is_empty() {
                                        Some(Ok(ChatCompletionMessage::default().add_tool_call(
                                            ToolCall::Part(ToolCallPart {
                                                call_id: Some(tool_call_id),
                                                name: Some(tool_name),
                                                arguments_part: call.arguments.clone(),
                                            }),
                                        )))
                                    } else {
                                        None
                                    }
                                }
                                oai::OutputItem::Reasoning(_reasoning) => {
                                    // Reasoning items don't emit content in real-time, only at
                                    // completion
                                    None
                                }
                                _ => None,
                            }
                        }
                        oai::ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(delta) => {
                            let (call_id, name) = state
                                .output_index_to_tool_call
                                .get(&delta.output_index)
                                .cloned()
                                .unwrap_or_else(|| {
                                    (
                                        ToolCallId::new(format!("output_{}", delta.output_index)),
                                        ToolName::new(""),
                                    )
                                });

                            let name = (!name.as_str().is_empty()).then_some(name);

                            Some(Ok(ChatCompletionMessage::default().add_tool_call(
                                ToolCall::Part(ToolCallPart {
                                    call_id: Some(call_id),
                                    name,
                                    arguments_part: delta.delta,
                                }),
                            )))
                        }
                        oai::ResponseStreamEvent::ResponseFunctionCallArgumentsDone(_done) => {
                            // Arguments are already sent via deltas, no need to emit here
                            None
                        }
                        oai::ResponseStreamEvent::ResponseCompleted(done) => {
                            let mut message = ChatCompletionMessage::default().finish_reason_opt(
                                Some(if state.output_index_to_tool_call.is_empty() {
                                    FinishReason::Stop
                                } else {
                                    FinishReason::ToolCalls
                                }),
                            );

                            if let Some(usage) = done.response.usage {
                                message = message.usage(usage.into_domain());
                            }

                            Some(Ok(message))
                        }
                        oai::ResponseStreamEvent::ResponseIncomplete(done) => {
                            let mut message = ChatCompletionMessage::default()
                                .finish_reason_opt(Some(FinishReason::Length));

                            if let Some(usage) = done.response.usage {
                                message = message.usage(usage.into_domain());
                            }

                            Some(Ok(message))
                        }
                        oai::ResponseStreamEvent::ResponseFailed(failed) => {
                            Some(Err(anyhow::anyhow!(
                                "Upstream response failed: {:?}",
                                failed.response.error
                            )))
                        }
                        oai::ResponseStreamEvent::ResponseError(err) => {
                            Some(Err(anyhow::anyhow!("Upstream error: {}", err.message)))
                        }
                        _ => None,
                    },
                    Err(err) => Some(Err(anyhow::Error::from(err))),
                };

                Some(item)
            })
        })
        .filter_map(|item| async move { item })
        .map(move |result| result.with_context(|| format_http_context(None, "POST", url.clone())))
}

/// Repository for OpenAI Codex models using the Responses API
///
/// Handles OpenAI's Codex models (e.g., gpt-5.1-codex, codex-mini-latest)
/// which use the Responses API instead of the standard Chat Completions API.
#[derive(Setters)]
#[setters(strip_option, into)]
pub struct OpenAIResponsesResponseRepository<F> {
    #[allow(dead_code)]
    infra: Arc<F>,
    retry_config: Arc<RetryConfig>,
}

impl<F> OpenAIResponsesResponseRepository<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self {
            infra,
            retry_config: Arc::new(RetryConfig::default()),
        }
    }
}

#[async_trait::async_trait]
impl<F: HttpInfra + 'static> ChatRepository for OpenAIResponsesResponseRepository<F> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: ChatContext,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let retry_config = self.retry_config.clone();
        let provider_client: OpenAIResponsesProvider<F> = OpenAIResponsesProvider::new(provider);
        let stream = provider_client
            .chat(model_id, context)
            .await
            .map_err(|e| into_retry(e, &retry_config))?;

        Ok(Box::pin(stream.map(move |item| {
            item.map_err(|e| into_retry(e, &retry_config))
        })))
    }

    async fn models(&self, _provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        // Codex models don't support model listing via the Responses API
        // Return empty list or hardcoded models
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_openai::types::responses as oai;
    use forge_app::domain::{
        Content, Context as ChatContext, ContextMessage, FinishReason, ModelId, Provider,
        ProviderId, ProviderResponse, ToolCallId, ToolChoice,
    };
    use tokio_stream::StreamExt;
    use url::Url;

    use super::*;
    use crate::provider::mock_server::MockServer;

    fn make_credential(provider_id: ProviderId, key: &str) -> Option<forge_domain::AuthCredential> {
        Some(forge_domain::AuthCredential {
            id: provider_id,
            auth_details: forge_domain::AuthDetails::ApiKey(forge_domain::ApiKey::from(
                key.to_string(),
            )),
            url_params: HashMap::new(),
        })
    }

    fn openai_responses(key: &str, url: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse(url).unwrap(),
            credential: make_credential(ProviderId::OPENAI, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: None,
        }
    }

    #[test]
    fn test_api_base_from_endpoint_url_trims_expected_suffixes() -> anyhow::Result<()> {
        let openai_endpoint = Url::parse("https://api.openai.com/v1/chat/completions")?;
        let openai_base = api_base_from_endpoint_url(&openai_endpoint)?;
        assert_eq!(openai_base.as_str(), "https://api.openai.com/v1");

        let copilot_endpoint = Url::parse("https://api.githubcopilot.com/chat/completions")?;
        let copilot_base = api_base_from_endpoint_url(&copilot_endpoint)?;
        assert_eq!(copilot_base.as_str(), "https://api.githubcopilot.com/v1");

        Ok(())
    }

    #[test]
    fn test_codex_request_from_context_converts_messages_tools_and_results() -> anyhow::Result<()> {
        let model = ModelId::from("codex-mini-latest");

        let tool_definition =
            forge_app::domain::ToolDefinition::new("shell").description("Run a shell command");

        let tool_call = forge_app::domain::ToolCallFull::new("shell")
            .call_id(ToolCallId::new("call_1"))
            .arguments(forge_app::domain::ToolCallArguments::from_json(
                r#"{"cmd":"echo hi"}"#,
            ));

        let tool_result = forge_app::domain::ToolResult::new("shell")
            .call_id(Some(ToolCallId::new("call_1")))
            .success("ok");

        let context = ChatContext::default()
            .add_message(ContextMessage::system("You are a helpful assistant."))
            .add_message(ContextMessage::user("Hello", None))
            .add_message(ContextMessage::assistant("", None, Some(vec![tool_call])))
            .add_message(ContextMessage::tool_result(tool_result))
            .add_tool(tool_definition)
            .tool_choice(ToolChoice::Auto)
            .max_tokens(123usize);

        let mut actual = oai::CreateResponse::from_domain(context)?;
        actual.model = Some(model.as_str().to_string());

        assert_eq!(actual.model.as_deref(), Some("codex-mini-latest"));
        assert_eq!(
            actual.instructions.as_deref(),
            Some("You are a helpful assistant.")
        );
        assert_eq!(actual.max_output_tokens, Some(123));

        let oai::InputParam::Items(items) = actual.input else {
            anyhow::bail!("Expected items input");
        };

        // user + function_call + function_call_output
        assert_eq!(items.len(), 3);

        let oai::InputItem::EasyMessage(user_msg) = &items[0] else {
            anyhow::bail!("Expected first item to be a user message");
        };
        assert_eq!(user_msg.role, oai::Role::User);

        let oai::InputItem::Item(oai::Item::FunctionCall(call)) = &items[1] else {
            anyhow::bail!("Expected second item to be a function call");
        };
        assert_eq!(call.call_id, "call_1");
        assert_eq!(call.name, "shell");

        let oai::InputItem::Item(oai::Item::FunctionCallOutput(out)) = &items[2] else {
            anyhow::bail!("Expected third item to be a function call output");
        };
        assert_eq!(out.call_id, "call_1");

        Ok(())
    }

    #[tokio::test]
    async fn test_into_chat_completion_message_codex_maps_text_and_finish() -> anyhow::Result<()> {
        let delta = oai::ResponseTextDeltaEvent {
            sequence_number: 1,
            item_id: "item_1".to_string(),
            output_index: 0,
            content_index: 0,
            delta: "hello".to_string(),
            logprobs: None,
        };

        let response: oai::Response = serde_json::from_value(serde_json::json!({
            "created_at": 0,
            "id": "resp_1",
            "model": "codex-mini-latest",
            "object": "response",
            "output": [],
            "status": "completed"
        }))?;

        let completed = oai::ResponseCompletedEvent { sequence_number: 2, response };

        let stream: oai::ResponseStream = Box::pin(tokio_stream::iter([
            Ok(oai::ResponseStreamEvent::ResponseOutputTextDelta(delta)),
            Ok(oai::ResponseStreamEvent::ResponseCompleted(completed)),
        ]));

        let url = Url::parse("https://api.openai.com/v1/chat/completions")?;
        let mut actual = into_chat_completion_message_codex(url, stream)
            .collect::<Vec<_>>()
            .await;

        let first = actual.remove(0)?;
        assert_eq!(first.content, Some(Content::part("hello")));

        let second = actual.remove(0)?;
        assert_eq!(second.finish_reason, Some(FinishReason::Stop));

        Ok(())
    }

    #[tokio::test]
    async fn test_openai_responses_provider_uses_responses_api_via_async_openai()
    -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;

        let response = serde_json::json!({
            "created_at": 0,
            "id": "resp_1",
            "model": "codex-mini-latest",
            "object": "response",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "role": "assistant",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": "hello",
                    "annotations": [],
                    "logprobs": null
                }]
            }],
            "status": "completed",
            "usage": {
                "input_tokens": 1,
                "output_tokens": 1,
                "total_tokens": 2,
                "input_tokens_details": {"cached_tokens": 0},
                "output_tokens_details": {"reasoning_tokens": 0}
            }
        });

        let mock = fixture.mock_responses(response, 200).await;

        let provider = openai_responses(
            "test-api-key",
            &format!("{}/v1/chat/completions", fixture.url()),
        );

        // Using a MockHttpClient for testing
        use bytes::Bytes;
        use reqwest::header::HeaderMap;
        use reqwest_eventsource::EventSource;

        #[derive(Clone)]
        struct MockHttpClient {
            client: reqwest::Client,
        }

        #[async_trait::async_trait]
        impl HttpInfra for MockHttpClient {
            async fn http_get(
                &self,
                url: &reqwest::Url,
                headers: Option<HeaderMap>,
            ) -> anyhow::Result<reqwest::Response> {
                let mut request = self.client.get(url.clone());
                if let Some(headers) = headers {
                    request = request.headers(headers);
                }
                Ok(request.send().await?)
            }

            async fn http_post(&self, _url: &reqwest::Url, _body: Bytes) -> anyhow::Result<reqwest::Response> {
                unimplemented!()
            }

            async fn http_delete(&self, _url: &reqwest::Url) -> anyhow::Result<reqwest::Response> {
                unimplemented!()
            }

            async fn http_eventsource(
                &self,
                _url: &reqwest::Url,
                _headers: Option<HeaderMap>,
                _body: Bytes,
            ) -> anyhow::Result<EventSource> {
                unimplemented!()
            }
        }

        let mock_http = Arc::new(MockHttpClient {
            client: reqwest::Client::new(),
        });

        let provider: OpenAIResponsesProvider<MockHttpClient> =
            OpenAIResponsesProvider::new(provider);
        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hi", None))
            .stream(false);

        let mut stream = provider
            .chat(&ModelId::from("codex-mini-latest"), context)
            .await?;

        let first = stream.next().await.expect("stream should yield")?;

        mock.assert_async().await;
        assert_eq!(first.content, Some(Content::full("hello")));
        assert_eq!(first.finish_reason, Some(FinishReason::Stop));

        Ok(())
    }
}