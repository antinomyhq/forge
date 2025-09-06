use anyhow::Context;
use bytes::Bytes;
use forge_app::HttpClientService;
use forge_app::domain::ChatCompletionMessage;
use forge_app::dto::openai::Error;
use reqwest::Url;
use reqwest::header::HeaderMap;
use reqwest_eventsource::{Event, EventSource};
use serde::de::DeserializeOwned;
use tokio_stream::{Stream, StreamExt};
use tracing::debug;

use super::utils::format_http_context;

pub fn into_chat_completion_message<Response>(
    url: Url,
    source: EventSource,
) -> impl Stream<Item = anyhow::Result<ChatCompletionMessage>>
where
    Response: DeserializeOwned,
    ChatCompletionMessage: TryFrom<Response, Error = anyhow::Error>,
{
    source
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
                                        "Failed to parse provider response: {}",
                                        message.data
                                    )
                                })
                                .and_then(|response| {
                                    ChatCompletionMessage::try_from(response).with_context(
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
                    .map(|result| result.with_context(|| format_http_context(None, "POST", url.clone())))
            })
}

pub async fn into_chat_completion_message_post<Response, H>(
    url: Url,
    headers: HeaderMap,
    body: Bytes,
    http_client: &H,
) -> anyhow::Result<ChatCompletionMessage>
where
    Response: DeserializeOwned,
    ChatCompletionMessage: TryFrom<Response, Error = anyhow::Error>,
    H: HttpClientService,
{
    let response = http_client
        .post(&url, Some(headers), body)
        .await
        .with_context(|| format_http_context(None, "POST", &url))?;

    let status = response.status();
    let response_text = response
        .text()
        .await
        .with_context(|| format_http_context(Some(status), "POST", &url))?;

    if !status.is_success() {
        return Err(anyhow::anyhow!(response_text))
            .with_context(|| format_http_context(Some(status), "POST", &url));
    }

    let parsed_response: Response = serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to parse provider response: {}", response_text))?;

    ChatCompletionMessage::try_from(parsed_response)
        .with_context(|| format!("Failed to create completion message: {}", response_text))
}
