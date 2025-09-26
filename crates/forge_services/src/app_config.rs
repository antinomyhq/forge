use std::sync::Arc;

use forge_app::AppConfigService;
use forge_app::dto::AppConfig;

use crate::AppConfigRepository;

pub struct ForgeConfigService<R> {
    repository: Arc<R>,
}

impl<R: AppConfigRepository> ForgeConfigService<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl<R: AppConfigRepository> AppConfigService for ForgeConfigService<R> {
    async fn get_app_config(&self) -> Option<AppConfig> {
        self.repository.get_app_config().await.ok().flatten()
    }

    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        self.repository.set_app_config(config).await
    }
}
