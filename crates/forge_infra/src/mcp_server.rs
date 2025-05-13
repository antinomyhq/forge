use std::collections::HashMap;

use forge_services::McpServer;
use rmcp::model::{ClientInfo, Implementation};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use tokio::process::Command;

use crate::mcp_client::ForgeMcpClient;

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

pub struct ForgeMcpServer;
impl ForgeMcpServer {
    fn client_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: Default::default(),
            client_info: Implementation { name: "Forge".to_string(), version: VERSION.to_string() },
        }
    }
}

#[async_trait::async_trait]
impl McpServer for ForgeMcpServer {
    type Client = ForgeMcpClient;

    async fn connect_stdio(
        &self,
        name: &str,
        command: &str,
        env: HashMap<String, String>,
        args: Vec<String>,
    ) -> anyhow::Result<Self::Client> {
        let mut command = Command::new(command);

        for (key, value) in env {
            command.env(key, value);
        }

        command
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let client = self
            .client_info()
            .serve(TokioChildProcess::new(command.args(args))?)
            .await?;

        Ok(ForgeMcpClient::new(name, client))
    }

    async fn connect_sse(&self, name: &str, url: &str) -> anyhow::Result<Self::Client> {
        let transport = rmcp::transport::SseTransport::start(url).await?;
        let client = self.client_info().serve(transport).await?;

        Ok(ForgeMcpClient::new(name, client))
    }
}
