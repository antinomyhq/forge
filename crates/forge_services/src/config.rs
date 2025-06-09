use std::sync::Arc;

use bytes::Bytes;
use forge_app::{ConfigService, EnvironmentService};
use forge_domain::ForgeConfig;

use crate::{FsReadService, FsWriteService, Infrastructure};

pub struct ForgeConfigService<I> {
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeConfigService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn read(&self) -> anyhow::Result<ForgeConfig> {
        let env = self.infra.environment_service().get_environment();
        let config = self
            .infra
            .file_read_service()
            .read(env.forge_config().as_path())
            .await?;
        Ok(serde_json::from_slice(&config)?)
    }
    async fn write(&self, config: &ForgeConfig) -> anyhow::Result<()> {
        let env = self.infra.environment_service().get_environment();
        self.infra
            .file_write_service()
            .write(
                env.forge_config().as_path(),
                Bytes::from(serde_json::to_vec(config)?),
            )
            .await
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> ConfigService for ForgeConfigService<I> {
    async fn read(&self) -> anyhow::Result<ForgeConfig> {
        self.read().await
    }

    async fn write(&self, config: &ForgeConfig) -> anyhow::Result<()> {
        self.write(config).await
    }
}
