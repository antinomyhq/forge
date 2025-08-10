use std::sync::Arc;

use bytes::Bytes;
use forge_app::AppConfigService;
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
    async fn read_app_config(&self) -> anyhow::Result<AppConfig> {
        self.read().await
    }

    async fn write_app_config(&self, config: &AppConfig) -> anyhow::Result<()> {
        self.write(config).await
    }
}
