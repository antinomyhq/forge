use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, ChatService, Context as ChatContext, EnvironmentService, ForgeKey,
    KeyService, Model, ModelId, Provider, ResultStream,
};
use forge_provider::Client;

use crate::{Infrastructure, ProviderService};

#[derive(Clone)]
pub struct ForgeProviderService<I, K> {
    infra: Arc<I>,
    key_service: Arc<K>,
    retry_status_codes: Arc<Vec<u16>>,
}

impl<K: KeyService, I: Infrastructure> ForgeProviderService<I, K> {
    pub fn new(infra: Arc<I>, key_service: Arc<K>) -> Self {
        let env = infra.environment_service().get_environment();
        let retry_status_codes = Arc::new(env.retry_config.retry_status_codes);
        Self { key_service, infra, retry_status_codes }
    }
    async fn key(&self) -> Result<ForgeKey> {
        let forge_key = self
            .key_service
            .get()
            .await
            .context("User isn't logged in")?;
        Ok(forge_key)
    }
    async fn provider(&self) -> Result<Provider> {
        let key = self.key().await?;

        self.infra
            .provider_service()
            .get(Some(key))
            .context("User isn't logged in")
    }
}

#[async_trait::async_trait]
impl<K: KeyService, I: Infrastructure> ChatService for ForgeProviderService<I, K> {
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        let client = Client::new(self.provider().await?, self.retry_status_codes.clone())?;

        client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {model}"))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        let client = Client::new(self.provider().await?, self.retry_status_codes.clone())?;

        client.models().await
    }
}
