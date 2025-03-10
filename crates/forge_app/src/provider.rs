use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, Model, ModelId, ProviderService, ResultStream,
};
use forge_open_router::Client;

use crate::{EnvironmentService, Infrastructure};

pub struct ForgeProviderService {
    // The provider service implementation
    client: Client,
}

impl ForgeProviderService {
    pub fn new<F: Infrastructure>(infra: Arc<F>) -> Self {
        let infra = infra.clone();
        let provider = infra
            .environment_service()
            .get_environment()
            .provider
            .clone();
        Self { client: Client::new(provider).unwrap() }
    }
}

#[async_trait::async_trait]
impl ProviderService for ForgeProviderService {
    async fn chat(
        &self,
        model_id: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.client
            .chat(model_id, request)
            .await
            .with_context(|| format!("Failed to chat with model: {}", model_id))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        self.client.models().await
    }
}
