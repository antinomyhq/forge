use std::sync::Arc;

use forge_domain::Provider;

use crate::{AppConfigService, ProviderRegistry};

pub struct ProviderCoordinator<S> {
    services: Arc<S>,
}

impl<S: AppConfigService + ProviderRegistry> ProviderCoordinator<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    pub async fn get_provider(&self) -> anyhow::Result<Provider> {
        let mut config = self.services.read_app_config().await.unwrap_or_default();
        let provider = self.services.get_provider(config.clone()).await?;

        if !config.is_tracked {
            if let Some(auth_provider_id) = provider.auth_provider_id() {
                // We only update auth_provider_id if it's not set
                if config
                    .key_info
                    .as_ref()
                    .and_then(|v| v.auth_provider_id.as_ref())
                    .is_none()
                {
                    let mut key_info = config.key_info.unwrap_or_default();
                    key_info.auth_provider_id = Some(auth_provider_id);
                    config.key_info = Some(key_info);

                    self.services.write_app_config(&config).await.ok();
                }
            }
        }

        Ok(provider)
    }
}
