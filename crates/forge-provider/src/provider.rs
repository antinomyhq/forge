use std::pin::Pin;

use super::error::Result;
use crate::model::{Request, Response};
use crate::ResultStream;

pub type MessageStream<A> = Box<dyn futures::Stream<Item = Result<A>>>;

#[async_trait::async_trait]
pub(crate) trait InnerProvider {
    async fn chat(&self, request: Request) -> Result<Pin<ResultStream<Response>>>;
    async fn models(&self) -> Result<Vec<String>>;
}

pub struct Provider {
    provider: Box<dyn InnerProvider>,
}

impl Provider {
    pub async fn chat(&self, request: Request) -> Result<Pin<ResultStream<Response>>> {
        self.provider.chat(request).await
    }

    pub async fn models(&self) -> Result<Vec<String>> {
        self.provider.models().await
    }

    pub(crate) fn new(provider: impl InnerProvider + 'static) -> Self {
        Self { provider: Box::new(provider) }
    }
}
