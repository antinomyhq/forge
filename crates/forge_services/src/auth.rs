use std::sync::Arc;

use anyhow::{bail, Context};
use forge_app::AuthService;
use forge_domain::{ForgeKey, InitAuth, Provider, ProviderUrl};

use crate::HttpInfra;

const TOKEN_POLL_ROUTE: &str = "cli/auth/token/";
const AUTH_INIT_ROUTE: &str = "cli/auth/init";
const AUTH_CANCEL_ROUTE: &str = "cli/auth/cancel/";

#[derive(Default, Clone)]
pub struct ForgeAuthService<I> {
    infra: Arc<I>,
}

impl<I: HttpInfra> ForgeAuthService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn init(&self, provider_url: Option<ProviderUrl>) -> anyhow::Result<InitAuth> {
        let init_url = format!(
            "{}{AUTH_INIT_ROUTE}",
            provider_url
                .map(ProviderUrl::into_string)
                .unwrap_or(Provider::ANTINOMY_URL.to_string())
        );
        let resp = self.infra.get(&init_url).await?;
        if !resp.status().is_success() {
            bail!("Failed to initialize auth")
        }

        Ok(serde_json::from_slice(&resp.bytes().await?)?)
    }

    async fn login(
        &self,
        auth: &InitAuth,
        provider_url: Option<ProviderUrl>,
    ) -> anyhow::Result<ForgeKey> {
        let url = format!(
            "{}{TOKEN_POLL_ROUTE}{}",
            provider_url
                .map(ProviderUrl::into_string)
                .unwrap_or(Provider::ANTINOMY_URL.to_string()),
            auth.session_id
        );

        let response = self.infra.get(&url).await?;
        match response.status().as_u16() {
            200 => Ok(ForgeKey::from(
                serde_json::from_slice::<serde_json::Value>(&response.bytes().await?)?
                    .get("apiKey")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
                    .context("Key not found in response")?,
            )),
            202 => bail!("Login timeout"),
            _ => bail!("Failed to log in"),
        }
    }

    async fn cancel(
        &self,
        auth: &InitAuth,
        provider_url: Option<ProviderUrl>,
    ) -> anyhow::Result<()> {
        let url = format!(
            "{}{AUTH_CANCEL_ROUTE}{}",
            provider_url
                .map(ProviderUrl::into_string)
                .unwrap_or(Provider::ANTINOMY_URL.to_string()),
            auth.session_id,
        );

        // Delete the session if auth is already completed in another session.
        self.infra.delete(&url).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<I: HttpInfra> AuthService for ForgeAuthService<I> {
    async fn init_auth(&self, provider_url: Option<ProviderUrl>) -> anyhow::Result<InitAuth> {
        self.init(provider_url).await
    }

    async fn login(
        &self,
        auth: &InitAuth,
        provider_url: Option<ProviderUrl>,
    ) -> anyhow::Result<ForgeKey> {
        self.login(auth, provider_url).await
    }
    async fn cancel_auth(
        &self,
        auth: &InitAuth,
        provider_url: Option<ProviderUrl>,
    ) -> anyhow::Result<()> {
        self.cancel(auth, provider_url).await
    }
}
