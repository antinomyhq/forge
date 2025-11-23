use std::collections::BTreeMap;

use forge_app::McpServerInfra;
use forge_domain::McpServerConfig;

use crate::mcp_client::ForgeMcpClient;

#[derive(Clone)]
pub struct ForgeMcpServer;

#[async_trait::async_trait]
impl McpServerInfra for ForgeMcpServer {
    type Client = ForgeMcpClient;

    async fn connect(
        &self,
        config: McpServerConfig,
        env_vars: &BTreeMap<String, String>,
    ) -> anyhow::Result<Self::Client> {
        Ok(ForgeMcpClient::new(config, env_vars))
    }
}
