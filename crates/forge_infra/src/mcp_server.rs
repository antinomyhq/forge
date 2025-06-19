use forge_domain::McpServerConfig;
use forge_services::McpServerInfra;

use crate::mcp_client::ForgeMcpClient;

#[derive(Clone)]
pub struct ForgeMcpServer;

#[async_trait::async_trait]
impl McpServerInfra for ForgeMcpServer {
    type Client = ForgeMcpClient;

    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<Self::Client> {
        Ok(ForgeMcpClient::new(config))
    }
}
