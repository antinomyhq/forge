use forge_domain::McpServerConfig;
use forge_services::McpServer;

use crate::mcp_client::ForgeMcpClient;

#[derive(Clone)]
pub struct ForgeMcpServer;

#[async_trait::async_trait]
impl McpServer for ForgeMcpServer {
    type Client = ForgeMcpClient;

    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<Self::Client> {
        println!("{:?}", config);
        Ok(ForgeMcpClient::new(config))
    }
}
