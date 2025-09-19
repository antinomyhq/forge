use std::sync::Arc;

use bytes::Bytes;
use forge_app::AuthConfigService;
use forge_app::dto::AuthConfig;

use crate::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};

pub struct ForgeConfigService<I> {
    infra: Arc<I>,
}

impl<I: FileReaderInfra + FileWriterInfra + EnvironmentInfra> ForgeConfigService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn read(&self) -> anyhow::Result<AuthConfig> {
        let env = self.infra.get_environment();
        let config = self.infra.read(env.app_config().as_path()).await?;
        Ok(serde_json::from_slice(&config)?)
    }
    async fn write(&self, config: &AuthConfig) -> anyhow::Result<()> {
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
impl<I: FileReaderInfra + FileWriterInfra + EnvironmentInfra> AuthConfigService
    for ForgeConfigService<I>
{
    async fn get_auth_config(&self) -> Option<AuthConfig> {
        self.read().await.ok()
    }

    async fn set_auth_config(&self, config: &AuthConfig) -> anyhow::Result<()> {
        self.write(config).await
    }
}
