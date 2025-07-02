use std::sync::Arc;

use bytes::Bytes;
use forge_app::{ForgeConfig, GlobalConfigService};

use crate::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};

pub struct ForgeConfigService<I> {
    infra: Arc<I>,
}

impl<I: FileReaderInfra + FileWriterInfra + EnvironmentInfra> ForgeConfigService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn read(&self) -> anyhow::Result<ForgeConfig> {
        let env = self.infra.get_environment();
        let config = self.infra.read(env.global_config().as_path()).await?;
        Ok(serde_json::from_slice(&config)?)
    }
    async fn write(&self, config: &ForgeConfig) -> anyhow::Result<()> {
        let env = self.infra.get_environment();
        self.infra
            .write(
                env.global_config().as_path(),
                Bytes::from(serde_json::to_vec(config)?),
                false,
            )
            .await
    }
}

#[async_trait::async_trait]
impl<I: FileReaderInfra + FileWriterInfra + EnvironmentInfra> GlobalConfigService
    for ForgeConfigService<I>
{
    async fn read_global_config(&self) -> anyhow::Result<ForgeConfig> {
        self.read().await
    }

    async fn write_global_config(&self, config: &ForgeConfig) -> anyhow::Result<()> {
        self.write(config).await
    }
}
