use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_domain::{EnvironmentService, McpConfig, McpConfigManager, Scope};
use merge::Merge;

use crate::{FsReadService, FsWriteService, Infrastructure};

pub struct ForgeMcpManager<I> {
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeMcpManager<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    async fn read_or_default_config(&self, path: &Path) -> anyhow::Result<McpConfig> {
        let config = self.infra.file_read_service().read_utf8(path).await;
        if let Ok(config) = config {
            Ok(serde_json::from_str(&config)?)
        } else {
            Ok(McpConfig::default())
        }
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
        let user_config = env.mcp_user_config();
        let local_config = env.mcp_local_config();

        // NOTE: Config at lower levels has higher priority.

        let mut config = McpConfig::default();
        let local_config = self
            .read_or_default_config(&local_config)
            .await
            .context(format!("Invalid config at: {}", local_config.display()))?;
        config.merge(local_config);

        let user_config = self
            .read_or_default_config(&user_config)
            .await
            .context(format!("Invalid config at: {}", user_config.display()))?;
        config.merge(user_config);

        Ok(config)
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
