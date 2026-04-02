use std::sync::Arc;

use forge_app::{AuthService, EnvironmentInfra, HttpInfra, User, UserUsage};
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};

const USER_INFO_ROUTE: &str = "auth/user";
const USER_USAGE_ROUTE: &str = "auth/usage";

#[derive(Default, Clone)]
pub struct ForgeAuthService<I> {
    infra: Arc<I>,
}

impl<I: HttpInfra + EnvironmentInfra> ForgeAuthService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    async fn user_info(&self, api_key: &str) -> anyhow::Result<User> {
        let url = format!("{}{USER_INFO_ROUTE}", self.infra.get_config().services_url);

        let url = Url::parse(&url)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let response = self
            .infra
            .http_get(&url, Some(headers))
            .await?
            .error_for_status()?;

        Ok(serde_json::from_slice(&response.bytes().await?)?)
    }

    async fn user_usage(&self, api_key: &str) -> anyhow::Result<UserUsage> {
        let url = Url::parse(&format!(
            "{}{USER_USAGE_ROUTE}",
            self.infra.get_config().services_url
        ))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let response = self
            .infra
            .http_get(&url, Some(headers))
            .await?
            .error_for_status()?;

        Ok(serde_json::from_slice(&response.bytes().await?)?)
    }
}

#[async_trait::async_trait]
impl<I: HttpInfra + EnvironmentInfra> AuthService for ForgeAuthService<I> {
    async fn user_info(&self, api_key: &str) -> anyhow::Result<User> {
        self.user_info(api_key).await
    }

    async fn user_usage(&self, api_key: &str) -> anyhow::Result<UserUsage> {
        self.user_usage(api_key).await
    }
}
