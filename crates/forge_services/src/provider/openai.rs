use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_openai::Client as AsyncOpenAIClient;
use async_openai::config::OpenAIConfig;
use async_openai::traits::RequestOptionsBuilder as _;
use async_openai::types::responses as oai;
use forge_app::HttpClientService;
use forge_app::domain::{
    ChatCompletionMessage, Content, Context as ChatContext, ContextMessage, FinishReason, ModelId,
    ProviderId, ResultStream, Role, TokenCount, ToolCall, ToolCallId, ToolCallPart, ToolChoice,
    ToolName, Transformer, Usage,
};
use forge_app::dto::openai::{ListModelResponse, ProviderPipeline, Request, Response};
use forge_domain::Provider;
use futures::StreamExt;
use lazy_static::lazy_static;
use reqwest::header::AUTHORIZATION;
use tracing::{debug, info};
use url::Url;

use crate::provider::client::{create_headers, join_url};
use crate::provider::event::into_chat_completion_message;
use crate::provider::utils::{format_http_context, sanitize_headers};

#[derive(Clone)]
pub struct OpenAIProvider<H> {
    provider: Provider<Url>,
    http: Arc<H>,
}

impl<H: HttpClientService> OpenAIProvider<H> {
    pub fn new(provider: Provider<Url>, http: Arc<H>) -> Self {
        Self { provider, http }
    }

    // OpenRouter optional headers ref: https://openrouter.ai/docs/api-reference/overview#headers
    // - `HTTP-Referer`: Identifies your app on openrouter.ai
    // - `X-Title`: Sets/modifies your app's title
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

    /// Creates headers including Session-Id for zai and zai_coding providers
    fn get_headers_with_request(&self, request: &Request) -> Vec<(String, String)> {
        let mut headers = self.get_headers();
        // Add Session-Id header for zai and zai_coding providers
        if let Some(session_id) = &request.session_id
            && (self.provider.id == ProviderId::ZAI || self.provider.id == ProviderId::ZAI_CODING)
        {
            headers.push(("Session-Id".to_string(), session_id.clone()));
            debug!(
                provider = %self.provider.url,
                session_id = %session_id,
                "Added Session-Id header for zai provider"
            );
        }

        headers
    }

    async fn inner_chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let mut request = Request::from(context).model(model.clone());
        let mut pipeline = ProviderPipeline::new(&self.provider);
        request = pipeline.transform(request);

        let url = self.provider.url.clone();
        let headers = create_headers(self.get_headers_with_request(&request));

        info!(
            url = %url,
            model = %model,
            headers = ?sanitize_headers(&headers),
            message_count = %request.message_count(),
            message_cache_count = %request.message_cache_count(),
            "Connecting Upstream"
        );

        let json_bytes =
            serde_json::to_vec(&request).with_context(|| "Failed to serialize request")?;

        let es = self
            .http
            .eventsource(&url, Some(headers), json_bytes.into())
            .await
            .with_context(|| format_http_context(None, "POST", &url))?;

        let stream = into_chat_completion_message::<Response>(url, es);

        Ok(Box::pin(stream))
    }

    async fn inner_chat_codex(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let base_url = api_base_from_endpoint_url(&self.provider.url)?;
        let responses_url = responses_endpoint_from_api_base(&base_url);

        let api_key = self
            .provider
            .credential
            .as_ref()
            .map(|c| match &c.auth_details {
                forge_domain::AuthDetails::ApiKey(key) => key.as_str(),
                forge_domain::AuthDetails::OAuthWithApiKey { api_key, .. } => api_key.as_str(),
                forge_domain::AuthDetails::OAuth { tokens, .. } => tokens.access_token.as_str(),
            })
            .ok_or_else(|| anyhow::anyhow!("Provider credential is required for Codex models"))?;

        // The OpenAIConfig api_base must include the version prefix (e.g. `.../v1`).
        // We derive it from Forge's endpoint-style provider.url.
        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(base_url.as_str());

        let client = AsyncOpenAIClient::with_config(config);
        let headers = create_headers(self.get_headers());

        let stream_requested = context.stream.unwrap_or(true);
        let request = codex_request_from_context(model, context)?;

        info!(
            url = %responses_url,
            base_url = %base_url,
            model = %model,
            headers = ?sanitize_headers(&headers),
            message_count = %request_message_count(&request),
            stream = %stream_requested,
            "Connecting Upstream (Codex via Responses API)"
        );

        if stream_requested {
            let stream = client
                .responses()
                .headers(headers)
                .create_stream(request)
                .await
                .with_context(|| format_http_context(None, "POST", &responses_url))?;

            let stream = into_chat_completion_message_codex(responses_url.clone(), stream);

            Ok(Box::pin(stream))
        } else {
            let response = client
                .responses()
                .headers(headers)
                .create(request)
                .await
                .with_context(|| format_http_context(None, "POST", &responses_url))?;

            let message = codex_response_into_full_message(response)?;
            let stream = tokio_stream::iter([Ok(message)]);
            Ok(Box::pin(stream))
        }
    }

    async fn inner_models(&self) -> Result<Vec<forge_app::domain::Model>> {
        // For Vertex AI, load models from static JSON file using VertexProvider logic
        if self.provider.id == ProviderId::VERTEX_AI {
            debug!("Loading Vertex AI models from static JSON file");
            Ok(self.inner_vertex_models())
        } else {
            let models = self
                .provider
                .models()
                .ok_or_else(|| anyhow::anyhow!("Provider models configuration is required"))?;

            match models {
                forge_domain::ModelSource::Url(url) => {
                    debug!(url = %url, "Fetching models");
                    match self.fetch_models(url.as_str()).await {
                        Err(error) => {
                            tracing::error!(error = ?error, "Failed to fetch models");
                            anyhow::bail!(error)
                        }
                        Ok(response) => {
                            let data: ListModelResponse = serde_json::from_str(&response)
                                .with_context(|| format_http_context(None, "GET", url))
                                .with_context(|| "Failed to deserialize models response")?;
                            Ok(data.data.into_iter().map(Into::into).collect())
                        }
                    }
                }
                forge_domain::ModelSource::Hardcoded(models) => {
                    debug!("Using hardcoded models");
                    Ok(models.clone())
                }
            }
        }
    }

    async fn fetch_models(&self, url: &str) -> Result<String, anyhow::Error> {
        let headers = create_headers(self.get_headers());
        let url = join_url(url, "")?;
        info!(method = "GET", url = %url, headers = ?sanitize_headers(&headers), "Fetching Models");

        let response = self
            .http
            .get(&url, Some(headers))
            .await
            .with_context(|| format_http_context(None, "GET", &url))
            .with_context(|| "Failed to fetch the models")?;

        let status = response.status();
        let ctx_message = format_http_context(Some(status), "GET", &url);

        let response_text = response
            .text()
            .await
            .with_context(|| ctx_message.clone())
            .with_context(|| "Failed to decode response into text")?;

        if status.is_success() {
            Ok(response_text)
        } else {
            Err(anyhow::anyhow!(response_text))
                .with_context(|| ctx_message)
                .with_context(|| "Failed to fetch the models")
        }
    }

    /// Load Vertex AI models from static JSON file
    fn inner_vertex_models(&self) -> Vec<forge_app::domain::Model> {
        lazy_static! {
            static ref VERTEX_MODELS: Vec<forge_app::domain::Model> = {
                let models =
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../vertex.json"));
                serde_json::from_str(models).unwrap()
            };
        }
        VERTEX_MODELS.clone()
    }
}

/// Returns `true` when the model ID should be routed through the Codex
/// (Responses API) path.
///
/// Matching rules are intentionally conservative and deterministic: we only
/// match on the model ID string itself. Today, all Codex models include `codex`
/// in their ID (e.g. `codex-mini-latest`, `gpt-4.1-codex`).
fn is_codex_model(model: &ModelId) -> bool {
    model.as_str().to_ascii_lowercase().contains("codex")
}

/// Returns `true` if we should use the OpenAI Responses API path for this
/// provider + model.
///
/// Currently supported:
/// - OpenAI: Codex models (gpt-5.1-codex, codex-mini-latest, etc.)
/// - GitHub Copilot: Codex models (same pattern - per sst/opencode
///   implementation)
///
/// Other OpenAI-compatible providers may not implement `/responses`.
fn should_use_responses_api(provider: &Provider<Url>, model: &ModelId) -> bool {
    let is_supported_provider =
        provider.id == ProviderId::OPENAI || provider.id == ProviderId::GITHUB_COPILOT;
    is_supported_provider && is_codex_model(model)
}

/// Derives an API base URL suitable for `async-openai` from a configured
/// endpoint URL.
///
/// The OpenAI provider config in Forge is typically an endpoint URL (e.g.
/// `/v1/chat/completions`). `async-openai` expects a base URL (e.g. `/v1`) and
/// will append the specific endpoint path.
///
/// Special handling:
/// - OpenAI: strips `/chat/completions` to keep `/v1`
/// - GitHub Copilot: strips `/chat/completions` and adds `/v1` prefix
fn api_base_from_endpoint_url(endpoint: &Url) -> anyhow::Result<Url> {
    let segments: Vec<&str> = endpoint
        .path_segments()
        .map(|s| s.filter(|seg| !seg.is_empty()).collect())
        .unwrap_or_default();

    if segments.is_empty() {
        anyhow::bail!("Provider endpoint URL has no path segments: {endpoint}");
    }

    // Most OpenAI-compatible providers use the Chat Completions endpoint.
    // For those, derive the base by trimming `/chat/completions`.
    let to_trim = if segments.len() >= 2
        && segments[segments.len() - 2] == "chat"
        && segments[segments.len() - 1] == "completions"
    {
        2
    } else {
        1
    };

    if segments.len() < to_trim {
        anyhow::bail!("Provider endpoint URL path is too short: {endpoint}");
    }

    let base_segments = &segments[..segments.len() - to_trim];

    // GitHub Copilot needs /v1 prefix even though their endpoint URL doesn't
    // include it
    let base_path =
        if base_segments.is_empty() && endpoint.host_str() == Some("api.githubcopilot.com") {
            "/v1".to_string()
        } else if base_segments.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", base_segments.join("/"))
        };

    let mut base = endpoint.clone();
    base.set_path(&base_path);
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

fn codex_tool_choice(choice: ToolChoice) -> oai::ToolChoiceParam {
    match choice {
        ToolChoice::None => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::None),
        ToolChoice::Auto => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Auto),
        ToolChoice::Required => oai::ToolChoiceParam::Mode(oai::ToolChoiceOptions::Required),
        ToolChoice::Call(name) => {
            oai::ToolChoiceParam::Function(oai::ToolChoiceFunction { name: name.to_string() })
        }
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
fn codex_request_from_context(
    model: &ModelId,
    context: ChatContext,
) -> anyhow::Result<oai::CreateResponse> {
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

    let tool_choice = context.tool_choice.map(codex_tool_choice);

    let mut builder = oai::CreateResponseArgs::default();
    builder.model(model.as_str());
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

    builder.build().map_err(anyhow::Error::from)
}

fn codex_usage_into_domain(usage: oai::ResponseUsage) -> Usage {
    Usage {
        prompt_tokens: TokenCount::Actual(usage.input_tokens as usize),
        completion_tokens: TokenCount::Actual(usage.output_tokens as usize),
        total_tokens: TokenCount::Actual(usage.total_tokens as usize),
        cached_tokens: TokenCount::Actual(usage.input_tokens_details.cached_tokens as usize),
        cost: None,
    }
}

fn codex_response_into_full_message(
    response: oai::Response,
) -> anyhow::Result<ChatCompletionMessage> {
    let mut message = ChatCompletionMessage::default();

    if let Some(text) = response.output_text() {
        message = message.content_full(text);
    }

    let mut saw_tool_call = false;
    for item in &response.output {
        if let oai::OutputItem::FunctionCall(call) = item {
            saw_tool_call = true;
            message = message.add_tool_call(ToolCall::Part(ToolCallPart {
                call_id: Some(ToolCallId::new(call.call_id.clone())),
                name: Some(ToolName::new(call.name.clone())),
                arguments_part: call.arguments.clone(),
            }));
        }
    }

    if let Some(usage) = response.usage {
        message = message.usage(codex_usage_into_domain(usage));
    }

    message = message.finish_reason_opt(Some(if saw_tool_call {
        FinishReason::ToolCalls
    } else {
        FinishReason::Stop
    }));

    Ok(message)
}

#[derive(Default)]
struct CodexStreamState {
    item_id_to_tool_call: HashMap<String, (ToolCallId, ToolName)>,
    item_id_has_delta: HashSet<String>,
    saw_tool_call: bool,
}

fn into_chat_completion_message_codex(
    url: Url,
    stream: oai::ResponseStream,
) -> impl tokio_stream::Stream<Item = anyhow::Result<ChatCompletionMessage>> {
    stream
        .scan(CodexStreamState::default(), move |state, event| {
            futures::future::ready({
                let item: Option<anyhow::Result<ChatCompletionMessage>> = match event {
                    Ok(event) => match event {
                        oai::ResponseStreamEvent::ResponseOutputTextDelta(delta) => Some(Ok(
                            ChatCompletionMessage::assistant(Content::part(delta.delta)),
                        )),
                        oai::ResponseStreamEvent::ResponseOutputItemAdded(added) => {
                            match added.item {
                                oai::OutputItem::FunctionCall(call) => {
                                    state.saw_tool_call = true;

                                    let item_id =
                                        call.id.clone().unwrap_or_else(|| call.call_id.clone());
                                    let tool_call_id = ToolCallId::new(call.call_id);
                                    let tool_name = ToolName::new(call.name);

                                    state.item_id_to_tool_call.insert(
                                        item_id.clone(),
                                        (tool_call_id.clone(), tool_name.clone()),
                                    );

                                    // Some providers include initial arguments here (possibly
                                    // empty).
                                    Some(Ok(ChatCompletionMessage::default().add_tool_call(
                                        ToolCall::Part(ToolCallPart {
                                            call_id: Some(tool_call_id),
                                            name: Some(tool_name),
                                            arguments_part: call.arguments,
                                        }),
                                    )))
                                }
                                _ => None,
                            }
                        }
                        oai::ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(delta) => {
                            state.item_id_has_delta.insert(delta.item_id.clone());
                            let (call_id, name) = state
                                .item_id_to_tool_call
                                .get(&delta.item_id)
                                .cloned()
                                .unwrap_or_else(|| {
                                    (ToolCallId::new(delta.item_id.clone()), ToolName::new(""))
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
                        oai::ResponseStreamEvent::ResponseFunctionCallArgumentsDone(done) => {
                            // Only emit the final arguments if we didn't receive deltas, to avoid
                            // duplicating the JSON.
                            if state.item_id_has_delta.contains(&done.item_id) {
                                if let Some(name) = done.name
                                    && let Some((_, tool_name)) =
                                        state.item_id_to_tool_call.get_mut(&done.item_id)
                                    && tool_name.as_str().is_empty()
                                {
                                    *tool_name = ToolName::new(name);
                                }
                                None
                            } else {
                                state.saw_tool_call = true;

                                let (call_id, name) = state
                                    .item_id_to_tool_call
                                    .get(&done.item_id)
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        (ToolCallId::new(done.item_id.clone()), ToolName::new(""))
                                    });

                                let name = done
                                    .name
                                    .map(ToolName::new)
                                    .or_else(|| (!name.as_str().is_empty()).then_some(name));

                                Some(Ok(ChatCompletionMessage::default().add_tool_call(
                                    ToolCall::Part(ToolCallPart {
                                        call_id: Some(call_id),
                                        name,
                                        arguments_part: done.arguments,
                                    }),
                                )))
                            }
                        }
                        oai::ResponseStreamEvent::ResponseCompleted(done) => {
                            let mut message = ChatCompletionMessage::default().finish_reason_opt(
                                Some(if state.saw_tool_call {
                                    FinishReason::ToolCalls
                                } else {
                                    FinishReason::Stop
                                }),
                            );

                            if let Some(usage) = done.response.usage {
                                message = message.usage(codex_usage_into_domain(usage));
                            }

                            Some(Ok(message))
                        }
                        oai::ResponseStreamEvent::ResponseIncomplete(done) => {
                            let mut message = ChatCompletionMessage::default()
                                .finish_reason_opt(Some(FinishReason::Length));

                            if let Some(usage) = done.response.usage {
                                message = message.usage(codex_usage_into_domain(usage));
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
impl<T: HttpClientService> OpenAIProvider<T> {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        if should_use_responses_api(&self.provider, model) {
            self.inner_chat_codex(model, context).await
        } else {
            self.inner_chat(model, context).await
        }
    }

    pub async fn models(&self) -> Result<Vec<forge_app::domain::Model>> {
        self.inner_models().await
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use anyhow::Context;
    use bytes::Bytes;
    use forge_app::HttpClientService;
    use forge_app::domain::{Provider, ProviderId, ProviderResponse};
    use reqwest::header::HeaderMap;
    use reqwest_eventsource::EventSource;
    use tokio_stream::StreamExt;
    use url::Url;

    use super::*;
    use crate::provider::mock_server::{MockServer, normalize_ports};

    #[test]
    fn test_is_codex_model_matches_expected_ids() {
        let codex_1 = ModelId::from("codex-mini-latest");
        let codex_2 = ModelId::from("gpt-4.1-codex");
        let non_codex = ModelId::from("gpt-4o");

        assert!(is_codex_model(&codex_1));
        assert!(is_codex_model(&codex_2));
        assert!(!is_codex_model(&non_codex));
    }

    #[test]
    fn test_api_base_from_endpoint_url_trims_expected_suffixes() -> anyhow::Result<()> {
        let openai_endpoint = Url::parse("https://api.openai.com/v1/chat/completions")?;
        let openai_base = api_base_from_endpoint_url(&openai_endpoint)?;
        assert_eq!(openai_base.as_str(), "https://api.openai.com/v1");

        let zai_endpoint = Url::parse("https://api.z.ai/api/paas/v4/chat/completions")?;
        let zai_base = api_base_from_endpoint_url(&zai_endpoint)?;
        assert_eq!(zai_base.as_str(), "https://api.z.ai/api/paas/v4");

        let responses_endpoint = Url::parse("https://example.com/v1/responses")?;
        let responses_base = api_base_from_endpoint_url(&responses_endpoint)?;
        assert_eq!(responses_base.as_str(), "https://example.com/v1");

        Ok(())
    }

    // Test helper functions
    fn make_credential(provider_id: ProviderId, key: &str) -> Option<forge_domain::AuthCredential> {
        Some(forge_domain::AuthCredential {
            id: provider_id,
            auth_details: forge_domain::AuthDetails::ApiKey(forge_domain::ApiKey::from(
                key.to_string(),
            )),
            url_params: HashMap::new(),
        })
    }

    fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            credential: make_credential(ProviderId::OPENAI, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.openai.com/v1/models").unwrap(),
            )),
        }
    }

    fn zai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            credential: make_credential(ProviderId::ZAI, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            )),
        }
    }

    fn github_copilot(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::GITHUB_COPILOT,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.githubcopilot.com/chat/completions").unwrap(),
            credential: make_credential(ProviderId::GITHUB_COPILOT, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.githubcopilot.com/models").unwrap(),
            )),
        }
    }

    fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZAI_CODING,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            credential: make_credential(ProviderId::ZAI_CODING, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            )),
        }
    }

    #[test]
    fn test_should_use_responses_api_for_openai_and_copilot_codex() {
        let openai_provider = openai("key");
        let copilot_provider = github_copilot("key");
        let other_provider = zai("key");

        let codex = ModelId::from("gpt-5.1-codex-max");
        let non_codex = ModelId::from("gpt-4o");

        // OpenAI + codex → responses
        assert!(should_use_responses_api(&openai_provider, &codex));
        assert!(!should_use_responses_api(&openai_provider, &non_codex));

        // GitHub Copilot + codex → responses (per sst/opencode pattern)
        assert!(should_use_responses_api(&copilot_provider, &codex));
        assert!(!should_use_responses_api(&copilot_provider, &non_codex));

        // Other providers → never responses
        assert!(!should_use_responses_api(&other_provider, &codex));
    }

    fn anthropic(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ANTHROPIC,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::Anthropic),
            url: Url::parse("https://api.anthropic.com/v1/messages").unwrap(),
            credential: make_credential(ProviderId::ANTHROPIC, key),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                Url::parse("https://api.anthropic.com/v1/models").unwrap(),
            )),
        }
    }

    // Mock implementation of HttpClientService for testing

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

        let actual = codex_request_from_context(&model, context)?;

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

    #[derive(Clone)]
    struct StreamingHttpClient {
        client: reqwest::Client,
    }

    impl StreamingHttpClient {
        fn new() -> Self {
            Self { client: reqwest::Client::new() }
        }
    }

    #[async_trait::async_trait]
    impl HttpClientService for StreamingHttpClient {
        async fn get(
            &self,
            _url: &reqwest::Url,
            _headers: Option<HeaderMap>,
        ) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn post(
            &self,
            _url: &reqwest::Url,
            _body: Bytes,
        ) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn delete(&self, _url: &reqwest::Url) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn eventsource(
            &self,
            url: &reqwest::Url,
            headers: Option<HeaderMap>,
            body: Bytes,
        ) -> anyhow::Result<EventSource> {
            use reqwest_eventsource::RequestBuilderExt;

            let mut request = self.client.post(url.clone()).body(body);
            if let Some(headers) = headers {
                request = request.headers(headers);
            }

            request
                .eventsource()
                .with_context(|| format_http_context(None, "POST (EventSource)", url))
        }
    }

    #[tokio::test]
    async fn test_openai_provider_non_codex_still_uses_sse_path() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let event = serde_json::json!({
            "id": "chatcmpl_1",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {"role": "assistant", "content": "hello"},
                "finish_reason": null
            }]
        });
        let sse_body = format!("data: {}\n\ndata: [DONE]\n\n", event);

        let mock = fixture.mock_chat_completions_stream(sse_body, 200).await;

        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse(&format!("{}/v1/chat/completions", fixture.url()))?,
            credential: None,
            auth_methods: vec![],
            url_params: vec![],
            models: None,
        };

        let provider = OpenAIProvider::new(provider, Arc::new(StreamingHttpClient::new()));
        let context = ChatContext::default()
            .add_message(ContextMessage::user("Hi", None))
            .stream(true);

        let mut stream = provider.chat(&ModelId::from("gpt-4o"), context).await?;
        let first = stream.next().await.expect("stream should yield")?;

        mock.assert_async().await;
        assert_eq!(first.content, Some(Content::part("hello")));

        Ok(())
    }

    #[tokio::test]
    async fn test_openai_provider_codex_uses_responses_api_via_async_openai() -> anyhow::Result<()>
    {
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

        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse(&format!("{}/v1/chat/completions", fixture.url()))?,
            credential: make_credential(ProviderId::OPENAI, "test-api-key"),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: None,
        };

        let provider = OpenAIProvider::new(provider, Arc::new(MockHttpClient::new()));
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

    #[derive(Clone)]
    struct MockHttpClient {
        client: reqwest::Client,
    }

    impl MockHttpClient {
        fn new() -> Self {
            Self { client: reqwest::Client::new() }
        }
    }

    #[async_trait::async_trait]
    impl HttpClientService for MockHttpClient {
        async fn get(
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

        async fn post(
            &self,
            _url: &reqwest::Url,
            _body: Bytes,
        ) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn delete(&self, _url: &reqwest::Url) -> anyhow::Result<reqwest::Response> {
            unimplemented!()
        }

        async fn eventsource(
            &self,
            _url: &reqwest::Url,
            _headers: Option<HeaderMap>,
            _body: Bytes,
        ) -> anyhow::Result<EventSource> {
            unimplemented!()
        }
    }

    fn create_provider(base_url: &str) -> anyhow::Result<OpenAIProvider<MockHttpClient>> {
        let provider = Provider {
            id: ProviderId::OPENAI,
            provider_type: forge_domain::ProviderType::Llm,
            response: Some(ProviderResponse::OpenAI),
            url: reqwest::Url::parse(base_url)?,
            credential: make_credential(ProviderId::OPENAI, "test-api-key"),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(forge_domain::ModelSource::Url(
                reqwest::Url::parse(base_url)?.join("models")?,
            )),
        };

        Ok(OpenAIProvider::new(
            provider,
            Arc::new(MockHttpClient::new()),
        ))
    }

    fn create_mock_models_response() -> serde_json::Value {
        serde_json::json!({
            "data": [
                {
                    "id": "model-1",
                    "name": "Test Model 1",
                    "description": "A test model",
                    "context_length": 4096,
                    "supported_parameters": ["tools", "supports_parallel_tool_calls"]
                },
                {
                    "id": "model-2",
                    "name": "Test Model 2",
                    "description": "Another test model",
                    "context_length": 8192,
                    "supported_parameters": ["tools"]
                }
            ]
        })
    }

    fn create_error_response(message: &str, code: u16) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "message": message,
                "code": code
            }
        })
    }

    fn create_empty_response() -> serde_json::Value {
        serde_json::json!({ "data": [] })
    }

    #[tokio::test]
    async fn test_fetch_models_success() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_mock_models_response(), 200)
            .await;
        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await?;

        mock.assert_async().await;
        insta::assert_json_snapshot!(actual);
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_http_error_status() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_error_response("Invalid API key", 401), 401)
            .await;

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await;

        mock.assert_async().await;

        // Verify that we got an error
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_server_error() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture
            .mock_models(create_error_response("Internal Server Error", 500), 500)
            .await;

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await;

        mock.assert_async().await;

        // Verify that we got an error
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_models_empty_response() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let mock = fixture.mock_models(create_empty_response(), 200).await;

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await?;

        mock.assert_async().await;
        assert!(actual.is_empty());
        Ok(())
    }

    #[test]
    fn test_error_deserialization() -> Result<()> {
        let content = serde_json::to_string(&serde_json::json!({
          "error": {
            "message": "This endpoint's maximum context length is 16384 tokens",
            "code": 400
          }
        }))
        .unwrap();
        let message = serde_json::from_str::<Response>(&content)
            .with_context(|| "Failed to parse response")?;
        let message = ChatCompletionMessage::try_from(message.clone());

        assert!(message.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_detailed_error_message_included() -> anyhow::Result<()> {
        let mut fixture = MockServer::new().await;
        let detailed_error = create_error_response(
            "Authentication failed: API key is invalid or expired. Please check your API key.",
            401,
        );
        let mock = fixture.mock_models(detailed_error, 401).await;

        let provider = create_provider(&fixture.url())?;
        let actual = provider.models().await;

        mock.assert_async().await;
        assert!(actual.is_err());
        insta::assert_snapshot!(normalize_ports(format!("{:#?}", actual.unwrap_err())));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_zai_provider() -> anyhow::Result<()> {
        let provider = zai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should have Authorization and Session-Id headers
        assert_eq!(headers.len(), 2);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "Session-Id" && v == "test-conversation-id")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_zai_coding_provider() -> anyhow::Result<()> {
        let provider = zai_coding("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should have Authorization and Session-Id headers
        assert_eq!(headers.len(), 2);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "Session-Id" && v == "test-conversation-id")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_openai_provider() -> anyhow::Result<()> {
        let provider = openai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should only have Authorization header (no Session-Id for non-zai providers)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_zai_provider_no_session_id() -> anyhow::Result<()> {
        let provider = zai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request without session_id
        let request = Request::default();

        let headers = openai_provider.get_headers_with_request(&request);

        // Should only have Authorization header (no Session-Id when session_id is None)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_headers_with_request_anthropic_provider() -> anyhow::Result<()> {
        let provider = anthropic("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        // Create a request with session_id
        let request = Request {
            session_id: Some("test-conversation-id".to_string()),
            ..Default::default()
        };

        let headers = openai_provider.get_headers_with_request(&request);

        // Should only have Authorization header (no Session-Id for Anthropic providers)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }

    #[test]
    fn test_get_headers_fallback() -> anyhow::Result<()> {
        let provider = zai("test-key");
        let http_client = Arc::new(MockHttpClient::new());
        let openai_provider = OpenAIProvider::new(provider, http_client);

        let headers = openai_provider.get_headers();

        // Should only have Authorization header (fallback method doesn't add
        // Session-Id)
        assert_eq!(headers.len(), 1);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer test-key")
        );
        assert!(!headers.iter().any(|(k, _)| k == "Session-Id"));
        Ok(())
    }
}
