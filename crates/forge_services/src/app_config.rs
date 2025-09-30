use std::sync::Arc;

use bytes::Bytes;
use forge_app::AppConfigService;
use forge_app::domain::{AgentId, ModelId};
use forge_app::dto::AppConfig;

use crate::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};

pub struct ForgeConfigService<I> {
    infra: Arc<I>,
}

impl<I: FileReaderInfra + FileWriterInfra + EnvironmentInfra> ForgeConfigService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn read(&self) -> anyhow::Result<AppConfig> {
        let env = self.infra.get_environment();
        let config = self.infra.read(env.app_config().as_path()).await?;
        Ok(serde_json::from_slice(&config)?)
    }
    async fn write(&self, config: &AppConfig) -> anyhow::Result<()> {
        let env = self.infra.get_environment();
        self.infra
            .write(
                env.app_config().as_path(),
                Bytes::from(serde_json::to_vec(config)?),
                false,
            )
            .await
    }
}

#[async_trait::async_trait]
impl<I: FileReaderInfra + FileWriterInfra + EnvironmentInfra> AppConfigService
    for ForgeConfigService<I>
{
    async fn get_app_config(&self) -> Option<AppConfig> {
        self.read().await.ok()
    }

    async fn set_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        self.write(config).await
    }

    async fn get_default_model(&self) -> anyhow::Result<Option<ModelId>> {
        Ok(self.read().await?.default_model)
    }

    async fn set_default_model(&self, model: ModelId) -> anyhow::Result<()> {
        let mut config = self.read().await.unwrap_or_default();
        config.default_model = Some(model);
        self.write(&config).await
    }

    async fn get_default_agent(&self) -> anyhow::Result<Option<AgentId>> {
        Ok(self.read().await?.default_agent)
    }

    async fn set_default_agent(&self, agent: AgentId) -> anyhow::Result<()> {
        let mut config = self.read().await.unwrap_or_default();
        config.default_agent = Some(agent);
        self.write(&config).await
    }
}
