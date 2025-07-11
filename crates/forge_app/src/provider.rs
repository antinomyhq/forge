use std::sync::Arc;

use forge_domain::User;

use crate::{AppConfigService, ProviderRegistry, UserService};

pub struct ProviderCoordinator<S> {
    services: Arc<S>,
}

impl<S: AppConfigService + ProviderRegistry + UserService> ProviderCoordinator<S> {
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    pub async fn get_provider(&self) -> anyhow::Result<User> {
        let config = self.services.read_app_config().await.unwrap_or_default();
        let provider = self.services.get_provider(config.clone()).await?;
        let user = self.services.fetch_user(provider).await?;
        Ok(user)
    }
}
