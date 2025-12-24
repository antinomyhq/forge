use std::sync::Arc;

use forge_app::ProviderService;
use forge_domain::{
    AnyProvider, AuthCredential, ChatCompletionMessage, Context, MigrationResult, Model, ModelId,
    Provider, ProviderId, ProviderRepository, ResultStream,
};
use url::Url;

/// Service layer wrapper for ProviderRepository
/// This service delegates chat and models calls to the underlying repository
/// implementation
pub struct ForgeProviderService<R> {
    pub(crate) repository: Arc<R>,
}

impl<R> ForgeProviderService<R> {
    /// Creates a new ForgeProviderService instance
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl<R: ProviderRepository> ProviderService for ForgeProviderService<R> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.repository.chat(model_id, context, provider).await
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        self.repository.models(provider).await
    }

    async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
        self.repository.get_all_providers().await
    }

    async fn get_provider(&self, id: ProviderId) -> anyhow::Result<Provider<Url>> {
        self.repository.get_provider(id).await
    }

    async fn upsert_credential(&self, credential: AuthCredential) -> anyhow::Result<()> {
        self.repository.upsert_credential(credential).await
    }

    async fn remove_credential(&self, id: &ProviderId) -> anyhow::Result<()> {
        self.repository.remove_credential(id).await
    }

    async fn migrate_env_credentials(&self) -> anyhow::Result<Option<MigrationResult>> {
        self.repository.migrate_env_credentials().await
    }
}
