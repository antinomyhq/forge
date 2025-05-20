use std::sync::Arc;

use forge_domain::{ConfigService, ForgeKey, KeyService};

pub struct ForgeKeyService<C> {
    config_service: Arc<C>,
}

impl<C: ConfigService> ForgeKeyService<C> {
    pub fn new(config_service: Arc<C>) -> Self {
        Self { config_service }
    }

    // FIXME: Add cache
    async fn get(&self) -> Option<ForgeKey> {
        let config = self.config_service.read().await.ok()?;
        config.key_info
    }
    async fn set(&self, key: ForgeKey) -> anyhow::Result<()> {
        let mut config = self.config_service.read().await.unwrap_or_default();
        config.key_info = Some(key.clone());
        self.config_service.write(&config).await?;

        Ok(())
    }
    async fn delete(&self) -> anyhow::Result<()> {
        if let Ok(mut config) = self.config_service.read().await {
            config.key_info = None;
            self.config_service.write(&config).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<C: ConfigService> KeyService for ForgeKeyService<C> {
    async fn get(&self) -> Option<ForgeKey> {
        self.get().await
    }

    async fn set(&self, key: ForgeKey) -> anyhow::Result<()> {
        self.set(key).await
    }

    async fn delete(&self) -> anyhow::Result<()> {
        self.delete().await
    }
}
