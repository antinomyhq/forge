use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, ChatService, Context as ChatContext, KeyService, Model, ModelId,
    ResultStream,
};
use forge_provider::Client;

use crate::{Infrastructure, ProviderService};

#[derive(Clone)]
pub struct ForgeProviderService<I, K> {
    infra: Arc<I>,
    key_service: Arc<K>,
}

impl<K: KeyService, I: Infrastructure> ForgeProviderService<I, K> {
    pub fn new(infra: Arc<I>, key_service: Arc<K>) -> Self {
        Self { key_service, infra }
    }
    async fn key(&self) -> Result<String> {
        let forge_key = self
            .key_service
            .get()
            .await
            .context("No key found, please log in first.")?;
        Ok(forge_key.key)
    }
}

#[async_trait::async_trait]
impl<K: KeyService, I: Infrastructure> ChatService for ForgeProviderService<I, K> {
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let key = self.key().await?;
        let client = Client::new(self.infra.provider_service().get(&key))?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        let key = self.key().await?;

        let client = Client::new(self.infra.provider_service().get(&key))?;

        client.models().await
    }
}
