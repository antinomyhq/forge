use std::collections::BTreeMap;

use forge_services::McpServer;

use crate::mcp_client::{Connector, ForgeMcpClient};

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
        Ok(ForgeMcpClient::new(Connector::Stdio {
            command: command.to_string(),
            env,
            args,
        }))
    }

    async fn connect_sse(&self, url: &str) -> anyhow::Result<Self::Client> {
        Ok(ForgeMcpClient::new(Connector::Sse { url: url.to_string() }))
    }
}
