use std::sync::Arc;

use forge_app::{ConfigService, KeyService};
use forge_domain::ForgeKey;
use tokio::sync::Mutex;

pub struct ForgeKeyService<C> {
    config_service: Arc<C>,
    key: Arc<Mutex<Option<ForgeKey>>>,
}

impl<C: ConfigService> ForgeKeyService<C> {
    pub fn new(config_service: Arc<C>) -> Self {
        Self { config_service, key: Default::default() }
    }
    async fn get(&self) -> Option<ForgeKey> {
        let mut lock = self.key.lock().await;
        if let Some(key_info) = lock.clone() {
            Some(key_info.clone())
        } else {
            let key = self
                .config_service
                .read()
                .await
                .ok()
                .and_then(|v| v.key_info);
            *lock = key.clone();

            key
        }
    }
    async fn set(&self, key: ForgeKey) -> anyhow::Result<()> {
        let mut config = self.config_service.read().await.unwrap_or_default();
        config.key_info = Some(key.clone());
        self.config_service.write(&config).await?;
        self.key.lock().await.replace(key.clone());

        Ok(())
    }
    async fn delete(&self) -> anyhow::Result<()> {
        if let Ok(mut config) = self.config_service.read().await {
            config.key_info = None;
            self.config_service.write(&config).await?;
            self.key.lock().await.take();
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<C: ConfigService> KeyService for ForgeKeyService<C> {
    async fn get_key(&self) -> Option<ForgeKey> {
        self.get().await
    }

    async fn set_key(&self, key: ForgeKey) -> anyhow::Result<()> {
        self.set(key).await
    }

    async fn delete_key(&self) -> anyhow::Result<()> {
        self.delete().await
    }
}
