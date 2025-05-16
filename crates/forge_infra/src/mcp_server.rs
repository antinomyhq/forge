

use forge_domain::McpServerConfig;
use forge_services::McpServer;

use crate::mcp_client::ForgeMcpClient;

#[derive(Clone)]
pub struct ForgeMcpServer;

#[async_trait::async_trait]
impl McpServer for ForgeMcpServer {
    type Client = ForgeMcpClient;
    
    async fn connect(&self, config: McpServerConfig) -> anyhow::Result<Self::Client> {
        match config {
            McpServerConfig::Stdio(stdio) => {
                let command = stdio.command.unwrap_or_default();
                let args = stdio.args;
                let env = stdio.env.unwrap_or_default();
                Ok(ForgeMcpClient::new_stdio(command, env, args))
            },
            McpServerConfig::Sse(sse) => {
                let url = sse.url.unwrap_or_default();
                Ok(ForgeMcpClient::new_sse(url))
            }
        }
    }
}
