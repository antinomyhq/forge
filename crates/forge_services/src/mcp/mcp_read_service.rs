use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_domain::{EnvironmentService, McpServer, McpConfigReadService, McpConfig, Scope};

use crate::{FsReadService, FsWriteService, Infrastructure};

pub struct ForgeMcpReadService<I> {
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeMcpReadService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    async fn read_config(&self, path: &Path) -> anyhow::Result<McpConfig> {
        let config = self.infra.file_read_service().read_utf8(path).await?;
        Ok(serde_json::from_str(&config)?)
    }
    async fn config_path(&self, scope: Scope) -> anyhow::Result<PathBuf> {
        let env = self.infra.environment_service().get_environment();
        match scope {
            Scope::User => Ok(env.mcp_user_config()),
            Scope::Local => Ok(env.mcp_local_config()),
        }
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> McpConfigReadService for ForgeMcpReadService<I> {
    async fn read(&self) -> anyhow::Result<McpConfig> {
        let env = self.infra.environment_service().get_environment();
        let mut user_config = self
            .read_config(env.mcp_user_config().as_path())
            .await
            .unwrap_or_default();
        let local_config = self
            .read_config(env.mcp_local_config().as_path())
            .await
            .unwrap_or_default();
        user_config.mcp_servers.extend(local_config.mcp_servers);

        Ok(user_config)
    }

    async fn write(&self, name: &str, mcp_servers: &McpServer, scope: Scope) -> anyhow::Result<()> {
        let config_path = self.config_path(scope).await?;

        let mut config = self
            .read_config(config_path.as_path())
            .await
            .unwrap_or_default();
        config
            .mcp_servers
            .insert(name.to_string(), mcp_servers.clone());
        self.infra
            .file_write_service()
            .write(
                config_path.as_path(),
                Bytes::from(serde_json::to_string(&config)?),
            )
            .await?;
        Ok(())
    }

    async fn write_json(&self, name: &str, mcp_servers: &str, scope: Scope) -> anyhow::Result<()> {
        let server_config: McpServer = serde_json::from_str(mcp_servers)?;
        self.write(name, &server_config, scope).await
    }

    async fn remove(&self, name: &str, scope: Scope) -> anyhow::Result<()> {
        let config_path = self.config_path(scope).await?;

        let mut config = self
            .read_config(config_path.as_path())
            .await
            .unwrap_or_default();
        config.mcp_servers.remove(name);
        self.infra
            .file_write_service()
            .write(
                config_path.as_path(),
                Bytes::from(serde_json::to_string(&config)?),
            )
            .await?;
        Ok(())
    }

    async fn get(&self, name: &str) -> anyhow::Result<McpServer> {
        let config = self.read().await?;
        if let Some(server_config) = config.mcp_servers.get(name) {
            Ok(server_config.clone())
        } else {
            Err(anyhow::anyhow!("MCP server not found"))
        }
    }
}
