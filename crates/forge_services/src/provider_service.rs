use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use forge_app::ProviderService;
use forge_app::domain::{
    AnyProvider, ChatCompletionMessage, Model, ModelId, ProviderId, ResultStream,
};
use forge_domain::{
    AuthCredential, ChatRepository, Context, MigrationResult, Provider, ProviderRepository,
};
use tokio::sync::Mutex;
use url::Url;

/// Service layer wrapper for ProviderRepository that handles model caching
pub struct ForgeProviderService<R> {
    repository: Arc<R>,
    cached_models: Arc<Mutex<HashMap<ProviderId, Vec<Model>>>>,
}

impl<R> ForgeProviderService<R> {
    /// Creates a new ForgeProviderService instance
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            cached_models: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl<R: ChatRepository + ProviderRepository> ProviderService for ForgeProviderService<R> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        // Repository builds client on each call (no caching at repository level)
        self.repository.chat(model_id, context, provider).await
    }

    async fn models(&self, provider: Provider<Url>) -> Result<Vec<Model>> {
        let provider_id = provider.id.clone();

        // Check cache first
        {
            let models_guard = self.cached_models.lock().await;
            if let Some(cached_models) = models_guard.get(&provider_id) {
                return Ok(cached_models.clone());
            }
        }

        // Models not in cache, fetch from repository
        let models = self.repository.models(provider).await?;

        // Cache the models for this provider
        {
            let mut models_guard = self.cached_models.lock().await;
            models_guard.insert(provider_id, models.clone());
        }

        Ok(models)
    }

    async fn get_all_providers(&self) -> Result<Vec<AnyProvider>> {
        self.repository.get_all_providers().await
    }

    async fn get_provider(&self, id: ProviderId) -> Result<Provider<Url>> {
        self.repository.get_provider(id).await
    }

    async fn upsert_credential(&self, credential: AuthCredential) -> Result<()> {
        self.repository.upsert_credential(credential).await
    }

    async fn remove_credential(&self, id: &ProviderId) -> Result<()> {
        self.repository.remove_credential(id).await
    }

    async fn migrate_env_credentials(&self) -> Result<Option<MigrationResult>> {
        self.repository.migrate_env_credentials().await
    }
}
