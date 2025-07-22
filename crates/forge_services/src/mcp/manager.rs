use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_app::McpConfigManager;
use forge_app::domain::{McpConfig, Scope};
use merge::Merge;

use crate::{EnvironmentInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra, McpServerInfra};

pub struct ForgeMcpManager<I> {
    infra: Arc<I>,
}

impl<I: McpServerInfra + FileReaderInfra + FileInfoInfra + EnvironmentInfra> ForgeMcpManager<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    async fn read_config(&self, path: &Path) -> anyhow::Result<McpConfig> {
        let config = self.infra.read_utf8(path).await?;
        Ok(serde_json::from_str(&config)?)
    }
    async fn config_path(&self, scope: &Scope) -> anyhow::Result<PathBuf> {
        let env = self.infra.get_environment();
        match scope {
            Scope::User => Ok(env.mcp_user_config()),
            Scope::Local => Ok(env.mcp_local_config()),
        }
    }
}

#[async_trait::async_trait]
impl<I: McpServerInfra + FileReaderInfra + FileInfoInfra + EnvironmentInfra + FileWriterInfra>
    McpConfigManager for ForgeMcpManager<I>
{
    async fn read_mcp_config(&self) -> anyhow::Result<McpConfig> {
        let env = self.infra.get_environment();
        let paths = vec![
            // Configs at lower levels take precedence, so we read them in reverse order.
            env.mcp_user_config().as_path().to_path_buf(),
            env.mcp_local_config().as_path().to_path_buf(),
        ];
        let mut config = McpConfig::default();
        for path in paths {
            if self.infra.is_file(&path).await.unwrap_or_default() {
                let new_config = self.read_config(&path).await.context(format!(
                    "An error occurred while reading config at: {}",
                    path.display()
                ))?;
                config.merge(new_config);
            }
        }

        Ok(config)
    }

    async fn write_mcp_config(&self, config: &McpConfig, scope: &Scope) -> anyhow::Result<()> {
        self.infra
            .write(
                self.config_path(scope).await?.as_path(),
                Bytes::from(serde_json::to_string(config)?),
                true,
            )
            .await
    }
}
