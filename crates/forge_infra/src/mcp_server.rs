use std::collections::BTreeMap;

use forge_services::McpServer;

use crate::mcp_client::ForgeMcpClient;

#[derive(Clone)]
pub struct ForgeMcpServer;

#[async_trait::async_trait]
impl McpServer for ForgeMcpServer {
    type Client = ForgeMcpClient;

    async fn connect_stdio(
        &self,
        command: &str,
        env: BTreeMap<String, String>,
        args: Vec<String>,
    ) -> anyhow::Result<Self::Client> {
        Ok(ForgeMcpClient::new_stdio(command.to_string(), env, args))
    }

    async fn connect_sse(&self, url: &str) -> anyhow::Result<Self::Client> {
        Ok(ForgeMcpClient::new_sse(url.to_string()))
    }
}
