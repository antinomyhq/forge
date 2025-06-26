use std::sync::Arc;

use anyhow::bail;
use forge_app::AuthService;
use forge_domain::{InitAuth, LoginInfo, Provider};

use crate::{EnvironmentInfra, HttpInfra};

const AUTH_ROUTE: &str = "cli/auth/";
const AUTH_INIT_ROUTE: &str = "cli/auth/init";

#[derive(Default, Clone)]
pub struct ForgeAuthService<I> {
    infra: Arc<I>,
}

impl<I: HttpInfra + EnvironmentInfra> ForgeAuthService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
    async fn init(&self) -> anyhow::Result<InitAuth> {
        let init_url = format!(
            "{}{AUTH_INIT_ROUTE}",
            self.infra
                .get_env_var("FORGE_API_URL")
                .unwrap_or(Provider::ANTINOMY_URL.to_string())
        );
        let resp = self.infra.get(&init_url).await?;
        if !resp.status().is_success() {
            bail!("Failed to initialize auth")
        }

        Ok(serde_json::from_slice(&resp.bytes().await?)?)
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<LoginInfo> {
        let url = format!(
            "{}{AUTH_ROUTE}{}",
            self.infra
                .get_env_var("FORGE_API_URL")
                .unwrap_or(Provider::ANTINOMY_URL.to_string()),
            auth.session_id
        );

        let response = self.infra.get(&url).await?;
        match response.status().as_u16() {
            200 => Ok(serde_json::from_slice::<LoginInfo>(
                &response.bytes().await?,
            )?),
            202 => bail!("Login timeout"),
            _ => bail!("Failed to log in"),
        }
    }

    async fn cancel(&self, auth: &InitAuth) -> anyhow::Result<()> {
        let url = format!(
            "{}{AUTH_ROUTE}{}",
            self.infra
                .get_env_var("FORGE_API_URL")
                .unwrap_or(Provider::ANTINOMY_URL.to_string()),
            auth.session_id,
        );

        // Delete the session if auth is already completed in another session.
        self.infra.delete(&url).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<I: HttpInfra + EnvironmentInfra> AuthService for ForgeAuthService<I> {
    async fn init_auth(&self) -> anyhow::Result<InitAuth> {
        self.init().await
    }

    async fn login(&self, auth: &InitAuth) -> anyhow::Result<LoginInfo> {
        self.login(auth).await
    }
    async fn cancel_auth(&self, auth: &InitAuth) -> anyhow::Result<()> {
        self.cancel(auth).await
    }
}
