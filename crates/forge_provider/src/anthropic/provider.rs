use std::time::Duration;

use anyhow::Context as _;
use derive_builder::Builder;
use forge_domain::{
    ChatCompletionMessage, Context, Model, ModelId, ProviderService, ResultStream, RetryConfig,
    RETRY_STATUS_CODES,
};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use reqwest_eventsource::{Event, RequestBuilderExt};
use tokio_stream::StreamExt;
use tracing::{debug, error};

use super::request::Request;
use super::response::{EventData, ListModelResponse};
use crate::retry::StatusCodeRetryPolicy;

#[derive(Clone, Builder)]
pub struct Anthropic {
    client: Client,
    api_key: String,
    base_url: Url,
    anthropic_version: String,
    #[builder(default = "RetryConfig::default()")]
    retry_config: RetryConfig,
}

impl Anthropic {
    pub fn builder() -> AnthropicBuilder {
        AnthropicBuilder::default()
    }

    fn url(&self, path: &str) -> anyhow::Result<Url> {
        // Validate the path doesn't contain certain patterns
        if path.contains("://") || path.contains("..") {
            anyhow::bail!("Invalid path: Contains forbidden patterns");
        }

        // Remove leading slash to avoid double slashes
        let path = path.trim_start_matches('/');

        self.base_url
            .join(path)
            .with_context(|| format!("Failed to append {} to base URL: {}", path, self.base_url))
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        // note: anthropic api requires the api key to be sent in `x-api-key` header.
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(self.api_key.as_str()).unwrap(),
        );

        // note: `anthropic-version` header is required by the API.
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(&self.anthropic_version).unwrap(),
        );
        headers
    }
}

#[async_trait::async_trait]
impl ProviderService for Anthropic {
    async fn chat(
        &self,
        model: &ModelId,
        context: Context,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let max_tokens = context.max_tokens.unwrap_or(4000);
        let request = Request::try_from(context)?
            .model(model.as_str().to_string())
            .stream(true)
            .max_tokens(max_tokens as u64);

        let url = self.url("/messages")?;
        debug!(url = %url, model = %model, "Connecting Upstream");
        let mut es = self
            .client
            .post(url)
            .headers(self.headers())
            .json(&request)
            .eventsource()?;
        let status_codes = self
            .retry_config
            .retry_status_codes
            .clone()
            .unwrap_or_else(|| RETRY_STATUS_CODES.to_vec());

        es.set_retry_policy(Box::new(StatusCodeRetryPolicy::new(
            Duration::from_millis(self.retry_config.initial_backoff_ms.unwrap_or(200)),
            self.retry_config.backoff_factor.unwrap_or(2) as f64,
            None, // No maximum duration
            self.retry_config.max_retry_attempts,
            status_codes,
        )));
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
                            serde_json::from_str::<EventData>(&message.data)
                                .with_context(|| "Failed to parse Anthropic event")
                                .and_then(|event| {
                                    ChatCompletionMessage::try_from(event).with_context(|| {
                                        format!(
                                            "Failed to create completion message: {}",
                                            message.data
                                        )
                                    })
                                }),
                        ),
                    },
                    Err(error) => match error {
                        reqwest_eventsource::Error::StreamEnded => None,
                        reqwest_eventsource::Error::InvalidStatusCode(_, response) => {
                            let headers = response.headers().clone();
                            let status = response.status();
                             match response.text().await {
                                Ok(ref body) => {
                                    debug!(status = ?status, headers = ?headers, body = body, "Invalid status code");
                                    Some(Err(anyhow::anyhow!("Invalid status code: {}, reason: {}", status, body)))
                                }
                                Err(error) => {
                                    error!(status = ?status, headers = ?headers, body = ?error, "Invalid status code (body not available)");
                                    Some(Err(anyhow::anyhow!("Invalid status code: {}", status)))
                                }
                            }
                        }
                        reqwest_eventsource::Error::InvalidContentType(_, ref response) => {
                            debug!(response = ?response, "Invalid content type");
                            Some(Err(error.into()))
                        }
                        error => {
                            debug!(error = %error, "Failed to receive chat completion event");
                            Some(Err(error.into()))
                        }
                    },
                }
            });

        Ok(Box::pin(stream.filter_map(|x| x)))
    }
    async fn models(&self) -> anyhow::Result<Vec<Model>> {
        let url = self.url("models")?;
        debug!(url = %url, "Fetching models");

        let result = self.client.get(url).headers(self.headers()).send().await;

        match result {
            Err(err) => {
                debug!(error = %err, "Failed to fetch models");
                anyhow::bail!(err)
            }
            Ok(response) => {
                let text = response
                    .error_for_status()
                    .with_context(|| "Failed because of a non 200 status code".to_string())?
                    .text()
                    .await?;
                let response: ListModelResponse = serde_json::from_str(&text)?;
                Ok(response.data.into_iter().map(Into::into).collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        Context, ContextMessage, ToolCallFull, ToolCallId, ToolChoice, ToolName, ToolResult,
    };

    use super::*;

    #[tokio::test]
    async fn test_url_for_models() {
        let anthropic = Anthropic::builder()
            .client(Client::new())
            .base_url(Url::parse("https://api.anthropic.com/v1/").unwrap())
            .anthropic_version("v1".to_string())
            .api_key("sk-some-key".to_string())
            .retry_config(RetryConfig::default())
            .build()
            .unwrap();
        assert_eq!(
            anthropic.url("/models").unwrap().as_str(),
            "https://api.anthropic.com/v1/models"
        );
    }

    #[tokio::test]
    async fn test_request_conversion() {
        let context = Context::default()
            .add_message(ContextMessage::system(
                "You're expert at math, so you should resolve all user queries.",
            ))
            .add_message(ContextMessage::user("what's 2 + 2 ?"))
            .add_message(ContextMessage::assistant(
                "here is the system call.",
                Some(vec![ToolCallFull {
                    name: ToolName::new("math"),
                    call_id: Some(ToolCallId::new("math-1")),
                    arguments: serde_json::json!({"expression": "2 + 2"}),
                }]),
            ))
            .add_tool_results(vec![ToolResult {
                name: ToolName::new("math"),
                call_id: Some(ToolCallId::new("math-1")),
                content: serde_json::json!({"result": 4}).to_string(),
                is_error: false,
            }])
            .tool_choice(ToolChoice::Call(ToolName::new("math")));
        let request = Request::try_from(context)
            .unwrap()
            .model("sonnet-3.5".to_string())
            .stream(true)
            .max_tokens(4000u64);
        insta::assert_snapshot!(serde_json::to_string_pretty(&request).unwrap());
    }
}
