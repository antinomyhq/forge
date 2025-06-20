use std::sync::Arc;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use forge_domain::{InitAuth, RetryConfig};

use crate::{AuthService, ConfigService, ProviderRegistry, Services};

pub struct Authenticator<S> {
    service: Arc<S>,
}

impl<S: Services> Authenticator<S> {
    pub fn new(service: Arc<S>) -> Self {
        Self { service }
    }
    pub async fn init(&self) -> anyhow::Result<InitAuth> {
        self.service.init_auth(self.service.provider_url()).await
    }
    pub async fn login(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        self.poll(
            RetryConfig::default()
                .max_retry_attempts(300usize)
                .max_delay(2)
                .backoff_factor(1u64),
            || self.login_inner(init_auth),
        )
        .await
    }
    pub async fn logout(&self) -> anyhow::Result<()> {
        let mut config = self.service.read().await?;
        config.key_info.take();
        self.service.write(&config).await?;
        Ok(())
    }
    async fn login_inner(&self, init_auth: &InitAuth) -> anyhow::Result<()> {
        let mut config = self.service.read().await?;
        if config.key_info.is_some() {
            self.service
                .cancel_auth(init_auth, self.service.provider_url())
                .await?;
        }
        let key = self
            .service
            .login(init_auth, self.service.provider_url())
            .await?;

        config.key_info.replace(key);
        self.service.write(&config).await?;
        Ok(())
    }
    async fn poll<T, F>(
        &self,
        config: RetryConfig,
        call: impl Fn() -> F + Send,
    ) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>> + Send,
    {
        let mut builder = ExponentialBuilder::default()
            .with_factor(config.backoff_factor as f32)
            .with_max_times(config.max_retry_attempts)
            .with_jitter();
        if let Some(max_delay) = config.max_delay {
            builder = builder.with_max_delay(Duration::from_secs(max_delay))
        }

        call.retry(builder).await
    }
}
