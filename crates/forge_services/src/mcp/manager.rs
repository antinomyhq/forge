use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_domain::{EnvironmentService, McpConfig, McpConfigManager, Scope};

use crate::{FsReadService, FsWriteService, Infrastructure};

pub struct ForgeMcpManager<I> {
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeMcpManager<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    async fn read_config(&self, path: &Path) -> anyhow::Result<McpConfig> {
        let config = self.infra.file_read_service().read_utf8(path).await?;
        Ok(serde_json::from_str(&config)?)
    }
    async fn config_path(&self, scope: &Scope) -> anyhow::Result<PathBuf> {
        let env = self.infra.environment_service().get_environment();
        match scope {
            Scope::User => Ok(env.mcp_user_config()),
            Scope::Local => Ok(env.mcp_local_config()),
        }
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> McpConfigManager for ForgeMcpManager<I> {
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

    async fn write(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()> {
        self.infra
            .file_write_service()
            .write(
                self.config_path(scope).await?.as_path(),
                Bytes::from(serde_json::to_string(config)?),
            )
            .await
    }
}
