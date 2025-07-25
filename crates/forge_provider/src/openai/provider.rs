use anyhow::{Context as _, Result};
use derive_builder::Builder;
use forge_app::domain::{
    ChatCompletionMessage, Context as ChatContext, ModelId, Provider, ContextMessage, ResultStream, Content
};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use reqwest_eventsource::{Event, RequestBuilderExt};
use tokio_stream::StreamExt;
use tracing::{debug, info};
use serde_json::{json, Value};
use super::model::{ListModelResponse, Model};
use super::request::Request;
use super::response::Response;
use crate::error::Error;
use crate::openai::transformers::{ProviderPipeline, Transformer};
use crate::utils::{format_http_context, sanitize_headers};
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::collections::HashMap;

#[derive(Clone, Builder)]
pub struct ForgeProvider {
    client: Client,
    provider: Provider,
    version: String,
}

static THREAD_ID_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

impl ForgeProvider {
    pub fn builder() -> ForgeProviderBuilder {
        ForgeProviderBuilder::default()
    }

    fn url(&self, path: &str) -> anyhow::Result<Url> {
        // Validate the path doesn't contain certain patterns
        if path.contains("://") || path.contains("..") {
            anyhow::bail!("Invalid path: Contains forbidden patterns");
        }

        // Remove leading slash to avoid double slashes
        let path = path.trim_start_matches('/');

        self.provider.to_base_url().join(path).with_context(|| {
            format!(
                "Failed to append {} to base URL: {}",
                path,
                self.provider.to_base_url()
            )
        })
    }

    async fn copilot_create_thread(&self) -> anyhow::Result<String> {
        let url = self.url("github/chat/threads")?;
        let headers = self.headers();
        let resp = self.client.post(url)
            .headers(headers)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;
        // Extract thread_id from response
        let thread_id = resp.get("thread_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No thread id in Copilot response"))?;
        Ok(thread_id.to_string())
    }

    // OpenRouter optional headers ref: https://openrouter.ai/docs/api-reference/overview#headers
    // - `HTTP-Referer`: Identifies your app on openrouter.ai
    // - `X-Title`: Sets/modifies your app's title
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(ref api_key) = self.provider.key() {
            if self.provider.is_copilot() {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("GitHub-Bearer {api_key}")).unwrap(),
                );            
                headers.insert(
                    "Copilot-Integration-Id",
                    HeaderValue::from_static("forge"),
                );
            } else {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {api_key}")).unwrap(),
                );
            }
        }

        headers.insert("X-Title", HeaderValue::from_static("forge"));
        headers.insert(
            "x-app-version",
            HeaderValue::from_str(format!("v{}", self.version).as_str())
                .unwrap_or(HeaderValue::from_static("v0.1.0-dev")),
        );
        headers.insert(
            "HTTP-Referer",
            HeaderValue::from_static("https://github.com/antinomyhq/forge"),
        );
        headers.insert(
            reqwest::header::CONNECTION,
            HeaderValue::from_static("keep-alive"),
        );
        debug!(headers = ?sanitize_headers(&headers), "Request Headers");
        headers
    }

    

    async fn copilot_chat(
        &self,
        model: &ModelId,
        context: ChatContext,
        thread_id: String,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        // 1. Extract the latest user message content
        let user_content = context
            .messages
            .last()
            .map(|m| m.to_text().clone())
            .unwrap_or_else(|| "".to_string());
    
        // 2. Build the Copilot request payload
        let payload = json!({
            "content": user_content,
            "context": context.messages.iter().map(|m| {
                match m {
                    ContextMessage::Text(text_message) => {
                        json!({
                            "role": text_message.role.to_string(),
                            "content": text_message.content,
                        })
                    },
                    ContextMessage::Tool(tool_message) => {
                        json!({
                            "role": "tool",
                            "content": format!("{:?}", tool_message.output),
                        })
                    },
                    ContextMessage::Image(image_message) => {
                        json!({
                            "role": "user",
                            "content": "[image omitted]",
                        })
                    }
                }
            }).collect::<Vec<_>>(),
            "intent": "conversation",
            "model": model.as_str(),
            "thread_id": thread_id,
            "streaming": true,
        });
    
        // 3. Build the URL
        let url = self.url(&format!("github/chat/threads/{}/messages", thread_id))?;
        let headers = self.headers();
        let url_for_filter = url.clone();
    
        // 4. Make the POST request and get the event stream
        let es = self
            .client
            .post(url.clone())
            .headers(headers)
            .json(&payload)
            .eventsource()
            .with_context(|| format_http_context(None, "POST", &url))?;
    
        // 5. Parse the event stream
        let stream = es
            .take_while(|message| {
                let is_stream_ended = matches!(message, Err(reqwest_eventsource::Error::StreamEnded));
                !is_stream_ended
            })
            .then(move |event| {
                async move {
                    match event {
                        Ok(Event::Message(ev)) => {
                            if let Ok(json) = serde_json::from_str::<Value>(&ev.data) {
                                match json.get("type").and_then(|t| t.as_str()) {
                                    Some("content") => {
                                        if let Some(body) = json.get("body").and_then(|b| b.as_str()) {
                                            let content = Content::part(body);
                                            Some(Ok(ChatCompletionMessage::assistant(content.clone())))
                                        } else {
                                            None
                                        }
                                    }
                                    Some("complete") => {
                                        // Stream is complete, no more messages to send
                                        None
                                    }
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        }
                        Ok(Event::Open) => {
                            None
                        }
                        Err(error) => {
                            match error {
                                reqwest_eventsource::Error::StreamEnded => None,
                                reqwest_eventsource::Error::InvalidStatusCode(_, response) => {
                                    let status = response.status();
                                    Some(Err(anyhow::anyhow!(Error::InvalidStatusCode(status.as_u16()))
                                        .context(format!("HTTP {}", status))))
                                }
                                reqwest_eventsource::Error::InvalidContentType(_, ref response) => {
                                    let status_code = response.status();
                                    Some(Err(anyhow::anyhow!(error)
                                        .context(format!("Invalid content type. HTTP Status: {}", status_code))))
                                }
                                error => {
                                    tracing::error!(error = ?error, "Failed to receive chat completion event");
                                    Some(Err(anyhow::anyhow!(error)))
                                }
                            }
                        }
                    }
                }
            })
            .filter_map(move |response| {
                response.map(|result| result.with_context(|| format_http_context(None, "POST", &url_for_filter)))
            });
        Ok(Box::pin(stream))
    }   

    async fn inner_chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> 
    {
        if self.provider.is_copilot() {
            let cid = context.conversation_id.clone().unwrap_or_default().to_string();
            // Check cache for thread_id
            let cached_tid = {
                let cache = THREAD_ID_CACHE.lock().unwrap();
                cache.get(&cid).cloned()
            };
            let thread_id = if let Some(tid) = cached_tid {
                tid
            } else {
                // Create thread outside lock
                let new_tid = self.copilot_create_thread().await?;
                // Insert into cache
                let mut cache = THREAD_ID_CACHE.lock().unwrap();
                cache.insert(cid, new_tid.clone());
                new_tid
            };
            return self.copilot_chat(model, context.clone(), thread_id).await;
        }
        let mut request = Request::from(context.clone()).model(model.clone()).stream(true);
        let mut pipeline = ProviderPipeline::new(&self.provider);
        request = pipeline.transform(request);
    
        let url = self.url("chat/completions")?;
        let headers = self.headers();

        info!(
            url = %url,
            model = %model,
            headers = ?sanitize_headers(&headers),
            message_count = %request.message_count(),
            message_cache_count = %request.message_cache_count(),
            "Connecting Upstream"
        );

        let es = self
            .client
            .post(url.clone())
            .headers(headers)
            .json(&request)
            .eventsource()
            .with_context(|| format_http_context(None, "POST", &url))?;
        let stream = es
            .take_while(|message| !matches!(message, Err(reqwest_eventsource::Error::StreamEnded)))
            .then(|event| async {
                match event {
                    Ok(event) => match event {
                        Event::Open => None,
                        Event::Message(event) if ["[DONE]", ""].contains(&event.data.as_str()) => {
                            debug!("Received completion from Upstream");
                            None
                        }
                        Event::Message(message) => Some(
                            serde_json::from_str::<Response>(&message.data)
                                .with_context(|| {
                                    format!(
                                        "Failed to parse Forge Provider response: {}",
                                        message.data
                                    )
                                })
                                .and_then(|response| {
                                    ChatCompletionMessage::try_from(response.clone()).with_context(
                                        || {
                                            format!(
                                                "Failed to create completion message: {}",
                                                message.data
                                            )
                                        },
                                    )
                                }),
                        ),
                    },
                    Err(error) => match error {
                        reqwest_eventsource::Error::StreamEnded => None,
                        reqwest_eventsource::Error::InvalidStatusCode(_, response) => {
                            let status = response.status();
                            let body = response.text().await.ok();
                            Some(Err(Error::InvalidStatusCode(status.as_u16())).with_context(
                                || match body {
                                    Some(body) => {
                                        format!("{status} Reason: {body}")
                                    }
                                    None => {
                                        format!("{status} Reason: [Unknown]")
                                    }
                                },
                            ))
                        }
                        reqwest_eventsource::Error::InvalidContentType(_, ref response) => {
                            let status_code = response.status();
                            debug!(response = ?response, "Invalid content type");
                            Some(Err(error).with_context(|| format!("Http Status: {status_code}")))
                        }
                        error => {
                            tracing::error!(error = ?error, "Failed to receive chat completion event");
                            Some(Err(error.into()))
                        }
                    },
                }
            })
            .filter_map(move |response| {
                response
                    .map(|result| result.with_context(|| format_http_context(None, "POST", &url)))
            });

        Ok(Box::pin(stream))
    }

    async fn inner_models(&self) -> Result<Vec<forge_app::domain::Model>> {
        let url = self.url("models")?;
        debug!(url = %url, "Fetching models");
        match self.fetch_models(url.clone()).await {
            Err(error) => {
                tracing::error!(error = ?error, "Failed to fetch models");
                anyhow::bail!(error)
            }
            Ok(response) => {
                let data: ListModelResponse = serde_json::from_str(&response)
                    .with_context(|| format_http_context(None, "GET", &url))
                    .with_context(|| "Failed to deserialize models response")?;
                Ok(data.data.into_iter().map(Into::into).collect())
            }
        }
    }

    async fn fetch_models(&self, url: Url) -> Result<String, anyhow::Error> {
        let headers = self.headers();
        info!(method = "GET", url = %url, headers = ?sanitize_headers(&headers), "Fetching Models");
        match self.client.get(url.clone()).headers(headers).send().await {
            Ok(response) => {
                let status = response.status();
                let ctx_message = format_http_context(Some(status), "GET", &url);
                let response = response
                    .text()
                    .await
                    .with_context(|| ctx_message.clone())
                    .with_context(|| "Failed to decode response into text")?;
                if status.is_success() {
                    Ok(response)
                } else {
                    // treat non 200 response as error.
                    Err(anyhow::anyhow!(response))
                        .with_context(|| ctx_message)
                        .with_context(|| "Failed to fetch the models")
                }
            }
            Err(err) => {
                let ctx_msg = format_http_context(err.status(), "GET", &url);
                Err(err)
                    .with_context(|| ctx_msg)
                    .with_context(|| "Failed to fetch the models")
            }
        }
    }
}

impl ForgeProvider {
    pub async fn chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.inner_chat(model, context).await
    }

    pub async fn models(&self) -> Result<Vec<forge_app::domain::Model>> {
        self.inner_models().await
    }
}

impl From<Model> for forge_app::domain::Model {
    fn from(value: Model) -> Self {
        let tools_supported = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "tools");
        let supports_parallel_tool_calls = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "supports_parallel_tool_calls");
        let is_reasoning_supported = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "reasoning");

        forge_app::domain::Model {
            id: value.id,
            name: value.name,
            description: value.description,
            context_length: value.context_length,
            tools_supported: Some(tools_supported),
            supports_parallel_tool_calls: Some(supports_parallel_tool_calls),
            supports_reasoning: Some(is_reasoning_supported),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use reqwest::Client;

    use super::*;
    use crate::mock_server::{MockServer, normalize_ports};

    fn create_provider(base_url: &str) -> anyhow::Result<ForgeProvider> {
        let provider = Provider::OpenAI {
            url: reqwest::Url::parse(base_url)?,
            key: Some("test-api-key".to_string()),
        };

        Ok(ForgeProvider::builder()
            .client(Client::new())
            .provider(provider)
            .version("1.0.0".to_string())
            .build()
            .unwrap())
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
}