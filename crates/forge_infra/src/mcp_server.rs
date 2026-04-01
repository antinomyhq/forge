use std::collections::BTreeMap;

use forge_app::McpServerInfra;
use forge_domain::{HttpConfig, McpServerConfig};

use crate::mcp_client::ForgeMcpClient;

#[derive(Clone)]
pub struct ForgeMcpServer {
    http_config: HttpConfig,
}

impl ForgeMcpServer {
    pub fn new(http_config: HttpConfig) -> Self {
        Self { http_config }
    }
}

#[async_trait::async_trait]
impl McpServerInfra for ForgeMcpServer {
    type Client = ForgeMcpClient;

    async fn connect(
        &self,
        config: McpServerConfig,
        env_vars: &BTreeMap<String, String>,
    ) -> anyhow::Result<Self::Client> {
        Ok(ForgeMcpClient::new(
            config,
            env_vars,
            self.http_config.clone(),
        ))
    }
}
