use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::{
    ChatCompletionMessage, Context as ChatContext, EnvironmentService, Model, ModelId,
    ProviderService, ResultStream,
};
use forge_provider::{Client, MockMode};

use crate::Infrastructure;

#[derive(Clone)]
pub struct ForgeProviderService {
    // The provider service implementation
    client: Arc<Client>,
}

impl ForgeProviderService {
    pub fn new<F: Infrastructure>(infra: Arc<F>) -> Self {
        let infra = infra.clone();
        let env = infra.environment_service().get_environment();
        let provider = env.provider.clone();
        let retry_config = env.retry_config;
        
        // Check if we should use a mock client
        let use_mock = std::env::var("FORGE_MOCK_PROVIDER").unwrap_or_default() == "true";
        let update_cache = std::env::var("FORGE_UPDATE_MOCK_CACHE").unwrap_or_default() == "true";
        
        let client = if use_mock {
            let mock_mode = if std::env::var("FORGE_OFFLINE_MODE").unwrap_or_default() == "true" {
                MockMode::Mock
            } else {
                MockMode::Real
            };
            
            let cache_dir = std::env::var("FORGE_MOCK_CACHE_DIR")
                .ok()
                .map(PathBuf::from);
                
            Client::with_mock(provider, retry_config, mock_mode, cache_dir, update_cache)
        } else {
            Client::new(provider, retry_config)
        };
        
        Self {
            client: Arc::new(client.unwrap()),
        }
    }
    
    #[cfg(test)]
    pub fn with_mock(provider: forge_domain::Provider, mock_mode: MockMode) -> Self {
        let retry_config = forge_domain::RetryConfig::default();
        let cache_dir = PathBuf::from("tests/fixtures/http_cache");
        
        Self {
            client: Arc::new(
                Client::with_mock(provider, retry_config, mock_mode, Some(cache_dir), false)
                    .unwrap(),
            ),
        }
    }
}

#[async_trait::async_trait]
impl ProviderService for ForgeProviderService {
    async fn chat(
        &self,
        model: &ModelId,
        request: ChatContext,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.client
            .chat(model, request)
            .await
            .with_context(|| format!("Failed to chat with model: {}", model))
    }

    async fn models(&self) -> Result<Vec<Model>> {
        self.client.models().await
    }
}
