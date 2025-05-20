use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use backon::{ExponentialBuilder, Retryable};
use forge_domain::{AuthService, InitAuth, KeyService, Provider};

use crate::{HttpService, Infrastructure};

#[derive(Default, Clone)]
pub struct ForgeAuthService<I, K> {
    infra: Arc<I>,
    key_service: Arc<K>,
}

impl<I: Infrastructure, K: KeyService> ForgeAuthService<I, K> {
    pub fn new(infra: Arc<I>, key_service: Arc<K>) -> Self {
        Self { infra, key_service }
    }
    async fn init(&self) -> anyhow::Result<InitAuth> {
        let init_url = format!("{}cli/auth/init", Provider::ANTINOMY_URL);
        let resp = self.infra.http_service().get(&init_url).await?;

        Ok(serde_json::from_slice(&resp)?)
    }

    // TODO: move this to infra.
    async fn poll<T, F>(&self, call: impl Fn() -> F) -> anyhow::Result<T>
    where
        F: Future<Output = anyhow::Result<T>>,
    {
        call.retry(
            ExponentialBuilder::default()
                .with_factor(1f32)
                .with_max_delay(Duration::from_secs(2))
                .with_max_times(300)
                .with_jitter(),
        )
        .await
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<()> {
        let poll_url = format!(
            "{}cli/auth/token/{}",
            Provider::ANTINOMY_URL,
            auth.session_id
        );

        // FIXME: we should call cli/auth/token/{session_id} to get the token
        // and send the token back to cli/auth/complete/{session_id} and expect
        // `ForgeKey` in response. NOTE that this needs change in the backend.

        self.key_service
            .set(serde_json::from_slice(
                &self.infra.http_service().get(&poll_url).await?,
            )?)
            .await
    }
    async fn logout(&self) -> anyhow::Result<()> {
        self.key_service.delete().await
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure, C: KeyService> AuthService for ForgeAuthService<I, C> {
    async fn init(&self) -> anyhow::Result<InitAuth> {
        self.init().await
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<()> {
        self.poll(|| self.login(auth))
            .await
            .context("Failed to log in")?;

        Ok(())
    }

    async fn logout(&self) -> anyhow::Result<()> {
        self.logout().await
    }
}
