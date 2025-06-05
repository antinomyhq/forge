use anyhow::{Context as _, Result};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use derive_builder::Builder;
use forge_domain::{
    self, ChatCompletionMessage, Context as ChatContext, ModelId, Provider, ResultStream,
};
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, Url};
use reqwest_eventsource::{Event, RequestBuilderExt};
use sha2::{Digest, Sha256};
use tokio_stream::StreamExt;
use tracing::debug;

use super::model::{ListModelResponse, Model};
use super::request::Request;
use super::response::Response;
use crate::error::Error;
use crate::forge_provider::transformers::{ProviderPipeline, Transformer};
use crate::utils::format_http_context;

#[derive(Clone, Builder)]
pub struct ForgeProvider {
    client: Client,
    provider: Provider,
    version: String,
}

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

    // OpenRouter optional headers ref: https://openrouter.ai/docs/api-reference/overview#headers
    // - `HTTP-Referer`: Identifies your app on openrouter.ai
    // - `X-Title`: Sets/modifies your app's title
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(ref api_key) = self.provider.key() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}")).unwrap(),
            );
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
        headers
    }

    async fn inner_chat(
        &self,
        model: &ModelId,
        context: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let mut request = Request::from(context).model(model.clone()).stream(true);
        request = ProviderPipeline::new(&self.provider).transform(request);

        let url = self.url("chat/completions")?;
        let timestamp = chrono::Utc::now().to_rfc3339();
        let sig = Self::sign(&timestamp, &request);

        debug!(
            url = %url,
            model = %model,
            message_count = %request.message_count(),
            message_cache_count = %request.message_cache_count(),
            "Connecting Upstream"
        );

        let es = self
            .client
            .post(url.clone())
            .headers(self.headers())
            .header("X-Forge-Timestamp", timestamp)
            .header("X-Forge-Signature", sig)
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
                            debug!(error = %error, "Failed to receive chat completion event");
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

    async fn inner_models(&self) -> Result<Vec<forge_domain::Model>> {
        let url = self.url("models")?;
        debug!(url = %url, "Fetching models");
        match self.fetch_models(url.clone()).await {
            Err(err) => {
                debug!(error = %err, "Failed to fetch models");
                anyhow::bail!(err)
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
        match self
            .client
            .get(url.clone())
            .headers(self.headers())
            .send()
            .await
        {
            Ok(response) => {
                let ctx_message = format_http_context(Some(response.status()), "GET", &url);
                match response.error_for_status() {
                    Ok(response) => Ok(response
                        .text()
                        .await
                        .with_context(|| ctx_message)
                        .with_context(|| "Failed to decode response into text")?),
                    Err(err) => Err(err)
                        .with_context(|| ctx_message)
                        .with_context(|| "Failed because of a non 200 status code"),
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
    fn sign<T: AsRef<[u8]>>(timestamp: T, request: &Request) -> String {
        // hash the secret key so it never fails for key length.
        let secret_hash = Sha256::digest(obfstr::obfstr!(env!("FORGE_SECRET")));
        let mut mac = Hmac::<Sha256>::new_from_slice(secret_hash.as_slice()).unwrap();
        mac.update(timestamp.as_ref());
        mac.update(
            serde_json::to_string(&request)
                .unwrap_or_default()
                .as_bytes(),
        );

        let result = mac.finalize().into_bytes();

        BASE64_STANDARD.encode(result)
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

    pub async fn models(&self) -> Result<Vec<forge_domain::Model>> {
        self.inner_models().await
    }
}

impl From<Model> for forge_domain::Model {
    fn from(value: Model) -> Self {
        let tools_supported = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "tools");
        forge_domain::Model {
            id: value.id,
            name: value.name,
            description: value.description,
            context_length: value.context_length,
            tools_supported: Some(tools_supported),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

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
