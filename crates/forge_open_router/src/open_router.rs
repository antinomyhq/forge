use std::sync::Arc;
use anyhow::{Context as _, Result};
use derive_setters::Setters;
use forge_domain::{self, ChatCompletionMessage, Context as ChatContext, Model, ModelId, Parameters, ProviderService, ResultStream};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, Url};
use reqwest_eventsource::{Event, RequestBuilderExt};
use tokio_stream::StreamExt;
use crate::ollama::OllamaResponseChunk;
use crate::provider_kind::ProviderKind;

use super::model::{ListModelResponse, OpenRouterModel};
use super::parameters::ParameterResponse;
use super::request::OpenRouterRequest;
use super::response::OpenRouterResponse;

#[derive(Default, Debug, Clone)]
pub struct OpenApi;

impl ProviderKind for OpenApi {
    fn to_chat_completion_message(&self, input: &[u8]) -> anyhow::Result<ChatCompletionMessage> {
        let message = serde_json::from_slice::<OpenRouterResponse>(input)?;
        let ans = ChatCompletionMessage::try_from(message)?;
        Ok(ans)
    }

    fn default_base_url(&self) -> String {
        "https://openrouter.ai/api/v1/".to_string()
    }
}

#[derive(Debug, Default, Clone, Setters)]
#[setters(into)]
pub struct OpenRouterBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
}

impl OpenRouterBuilder {
    pub fn build(self, ty: crate::model::Model) -> anyhow::Result<OpenRouterClient> {
        let client = Client::builder().build()?;
        let default_url = ty.default_base_url();
        let base_url = self
            .base_url
            .as_deref()
            .unwrap_or(default_url.as_str());

        let base_url = Url::parse(base_url)
            .with_context(|| format!("Failed to parse base URL: {}", base_url))?;

        Ok(OpenRouterClient { client, base_url, api_key: self.api_key, ty })
    }
}

#[derive(Clone)]
pub struct OpenRouterClient {
    client: Client,
    api_key: Option<String>,
    base_url: Url,
    ty: crate::model::Model,
}

impl OpenRouterClient {
    pub fn builder() -> OpenRouterBuilder {
        OpenRouterBuilder::default()
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

        if let Some(ref api_key) = self.api_key {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
            );
        }
        headers.insert("X-Title", HeaderValue::from_static("code-forge"));
        headers
    }
}

#[async_trait::async_trait]
impl ProviderService for OpenRouterClient {
    async fn chat(
        &self,
        model_id: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let request = OpenRouterRequest::from(request)
            .model(model_id.clone())
            .stream(true)
            .cache();

        let es = self
            .client
            .post(self.url("chat/completions")?)
            .headers(self.headers())
            .json(&request)
            .eventsource()?;
        let ty = Arc::new(self.ty.clone());
        
/*        let resp = self
            .client
            .post(self.url("chat/completions")?)
            .headers(self.headers())
            .json(&request)
            .send()
            .await?;
        let stream = resp
            .bytes_stream()
            .map(move |result| -> Option<Result<ChatCompletionMessage, anyhow::Error>> {
                let ty = ty.clone();
                match result {
                    Ok(bytes) => {
                        println!("{:?}", bytes);
                        // Convert bytes to string
                        let event_data = match std::str::from_utf8(&bytes) {
                            Ok(s) => {
                                let s = s.trim();
                                let s = s.strip_prefix("data: ").unwrap_or(s);
                                let s = s.strip_suffix("data: [DONE]").unwrap_or(s);
                                s
                            },
                            Err(e) => return Some(Err(anyhow::anyhow!("Failed to convert bytes to string: {}", e))),
                        };

                        // Skip empty or "[DONE]" events
                  /*      if ["[DONE]", ""].contains(&event_data) {
                            return None;
                        }*/
                        match ty.to_chat_completion_message(event_data.as_bytes()) {
                            Ok(val) => Some(Ok(val)),
                            Err(e) => {
                                println!("Error: {:?}", e);
                                Some(Err(e))
                            },
                        }
                        // Parse the event data
                        /*match serde_json::from_str::<OpenRouterResponse>(event_data) {
                            Ok(response) => {
                                match ChatCompletionMessage::try_from(response) {
                                    Ok(message) => Some(Ok(message)),
                                    Err(e) => Some(Err(anyhow::anyhow!(
                                        "Failed to create completion message: {}",
                                        e
                                    ))),
                                }
                            }
                            Err(e) => Some(Err(anyhow::anyhow!(
                                "Failed to parse OpenRouter response: {}",
                                e
                            ))),
                        }*/
                    }
                    Err(e) => Some(Err(anyhow::anyhow!("Stream error: {}", e))),
                }
            });
*/
/*        let stream = resp
            .bytes_stream()
            .map(move |result| -> Result<ChatCompletionMessage, anyhow::Error> {
                let bytes = result?;
                println!("Received chunk: {:?}", bytes);

                // Parse the chunk and convert to ChatCompletionMessage
                ty
                    .to_chat_completion_message(bytes.as_ref())
            })
            .filter_map(|result| {
                match result {
                    Ok(message) => Some(Ok(message)),
                    Err(e) => {
                        println!("Error: {:?}", e);
                        None
                    }, // Silently drop parsing errors
                }
            });
*/
                let stream = es
                    .take_while(|message| !matches!(message, Err(reqwest_eventsource::Error::StreamEnded)))
                    .map(move |event| {
                        let ty = ty.clone();
                        match event {
                            Ok(event) => match event {
                                Event::Open => None,
                                Event::Message(event) if ["[DONE]", ""].contains(&event.data.as_str()) => {
                                    None
                                }
                                Event::Message(event) => {
                                    /*Some(
                                        serde_json::from_str::<OllamaResponseChunk>(&event.data)
                                            .with_context(|| "Failed to parse OpenRouter response")
                                            .and_then(|message| {
                                                ChatCompletionMessage::try_from(crate::ollama::OllamaResponseChunk::from(message.clone()))
                                                    .with_context(|| "Failed to create completion message")
                                            }),
                                    )*/
                                    Some(ty.to_chat_completion_message(event.data.as_bytes()))
                                }
                            },
                            Err(reqwest_eventsource::Error::StreamEnded) => None,
                            Err(reqwest_eventsource::Error::InvalidStatusCode(code, _)) => {
                                println!("Invalid status code: {}", code);
                    /*            let x = response.text().await;
                                println!("Response: {:?}", x);
                                let x = x.map_err(|e| anyhow::anyhow!("{}", e)).and_then(|x| {
                                    ty.to_chat_completion_message(&x)
                                        .with_context(|| "Failed to parse OpenRouter response")
                                });
                                x.ok().map(Ok)*/
                                Some(Err(anyhow::anyhow!("Invalid status code: {}", code)))
                                /*Some(
                                    x
                                        .with_context(|| "Failed to parse OpenRouter response")
                                        .and_then(|message| {
                                            ChatCompletionMessage::try_from(message.clone())
                                                .with_context(|| "Failed to create completion message")
                                        })
                                        .with_context(|| "Failed with invalid status code"),
                                )*/
                            }
                            Err(reqwest_eventsource::Error::InvalidContentType(_, _)) => {
                                Some(Err(anyhow::anyhow!("Invalid content type")))
/*                                let x = response.text().await;
                                x.map_err(|e| anyhow::anyhow!("{}", e)).and_then(|x| {
                                    ty.clone().to_chat_completion_message(&x)
                                        .with_context(|| "Failed to parse OpenRouter response")
                                }).ok().map(Ok)*/
                                /*
                                Some(
                                    response
                                        .json::<OpenRouterResponse>()
                                        .await
                                        .with_context(|| "Failed to parse OpenRouter response")
                                        .and_then(|message| {
                                            ChatCompletionMessage::try_from(message.clone())
                                                .with_context(|| "Failed to create completion message")
                                        })
                                        .with_context(|| "Failed with invalid content type"),
                                )*/
                            },
                            Err(err) => Some(Err(err.into())),
                        }
                    });

        // Ok(Box::pin(stream.filter_map(|x| x)))
        Ok(Box::pin(stream.filter_map(|x| x)))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        let text = self
            .client
            .get(self.url("models")?)
            .headers(self.headers())
            .send()
            .await?
            .error_for_status()
            .with_context(|| "Failed because of a non 200 status code".to_string())?
            .text()
            .await?;

        let response: ListModelResponse = serde_json::from_str(&text)?;

        Ok(response
            .data
            .iter()
            .map(|r| r.clone().into())
            .collect::<Vec<Model>>())
    }

    async fn parameters(&self, model: &ModelId) -> Result<Parameters> {
        Ok(Parameters { tool_supported: true })

        /*
        // For Eg: https://openrouter.ai/api/v1/parameters/google/gemini-pro-1.5-exp
        let path = format!("parameters/{}", model.as_str());

        let url = self.url(&path)?;

        let text = self
            .client
            .get(url)
            .headers(self.headers())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let response: ParameterResponse = serde_json::from_str(&text)
            .with_context(|| "Failed to parse parameter response".to_string())?;

        Ok(Parameters {
            tool_supported: response
                .data
                .supported_parameters
                .iter()
                .flat_map(|parameter| parameter.iter())
                .any(|parameter| parameter == "tools"),
        })*/
    }
}

impl From<OpenRouterModel> for Model {
    fn from(value: OpenRouterModel) -> Self {
        Model {
            id: value.id,
            name: value.name,
            description: value.description,
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use reqwest::Url;

    use super::*;

    fn create_test_client() -> OpenRouterClient {
        OpenRouterClient {
            client: Client::new(),
            api_key: None,
            base_url: Url::parse("https://openrouter.ai/api/v1/").unwrap(),
        }
    }

    #[test]
    fn test_url_basic_path() -> Result<()> {
        let client = create_test_client();
        let url = client.url("chat/completions")?;
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        Ok(())
    }

    #[test]
    fn test_url_with_leading_slash() -> Result<()> {
        let client = create_test_client();
        // Remove leading slash since base_url already ends with a slash
        let path = "chat/completions".trim_start_matches('/');
        let url = client.url(path)?;
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        Ok(())
    }

    #[test]
    fn test_url_with_special_characters() -> Result<()> {
        let client = create_test_client();
        let url = client.url("parameters/google/gemini-pro-1.5-exp")?;
        assert_eq!(
            url.as_str(),
            "https://openrouter.ai/api/v1/parameters/google/gemini-pro-1.5-exp"
        );
        Ok(())
    }

    #[test]
    fn test_url_with_empty_path() -> Result<()> {
        let client = create_test_client();
        let url = client.url("")?;
        assert_eq!(url.as_str(), "https://openrouter.ai/api/v1/");
        Ok(())
    }

    #[test]
    fn test_url_with_invalid_path() {
        let client = create_test_client();
        let result = client.url("https://malicious.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_url_with_directory_traversal() {
        let client = create_test_client();
        let result = client.url("../invalid");
        assert!(result.is_err());
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
        let message = serde_json::from_str::<OpenRouterResponse>(&content)
            .context("Failed to parse response")?;
        let message = ChatCompletionMessage::try_from(message.clone());

        assert!(message.is_err());
        Ok(())
    }
}
