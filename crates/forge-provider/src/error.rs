use std::pin::Pin;

use derive_more::derive::Display;
use serde_json::Value;

#[derive(Debug, Display, derive_more::From)]
pub enum Error {
    // Custom display message for provider error
    #[display("{}", error)]
    Provider {
        provider: String,
        error: ProviderError,
    },
    Reqwest(#[from] reqwest::Error),
    SerdeJson(#[from] serde_json::Error),
    EventSource(#[from] reqwest_eventsource::Error),
}

impl Error {
    pub fn empty_response(provider: impl Into<String>) -> Self {
        Error::Provider {
            provider: provider.into(),
            error: ProviderError::EmptyContent,
        }
    }
}

#[derive(Debug, Display)]
pub enum ProviderError {
    // Custom display message for OpenAI error
    // OpenAI(OpenAIError),

    // Custom display message for EmptyResponse
    EmptyContent,
    ToolUseEmptyName,
    UpstreamError(Value),
}

pub type Result<T> = std::result::Result<T, Error>;
pub type ResultStream<T> = Pin<Box<dyn tokio_stream::Stream<Item = Result<T>> + Send>>;
