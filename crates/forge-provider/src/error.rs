use std::pin::Pin;

use async_openai::error::OpenAIError;
use derive_more::derive::Display;

#[derive(Debug, Display, derive_more::From)]
pub enum Error {
    // Custom display message for provider error
    #[display("{}", error)]
    Provider {
        provider: String,
        error: ProviderError,
    },
    ReqwestMiddleware(#[from] reqwest_middleware::Error),
    Reqwest(#[from] reqwest_middleware::reqwest::Error),
    SerdeJson(#[from] serde_json::Error),
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
    OpenAI(OpenAIError),

    // Custom display message for EmptyResponse
    EmptyContent,
    ToolUseEmptyName,
}

pub type Result<T> = std::result::Result<T, Error>;
pub type ResultStream<T> = Pin<Box<dyn tokio_stream::Stream<Item = Result<T>>>>;

impl From<OpenAIError> for Error {
    fn from(error: OpenAIError) -> Self {
        Error::Provider {
            provider: "OpenAI".to_string(),
            error: ProviderError::OpenAI(error),
        }
    }
}
