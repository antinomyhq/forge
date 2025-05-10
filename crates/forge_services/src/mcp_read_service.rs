use std::path::Path;
use std::sync::Arc;

use forge_domain::{EnvironmentService, McpConfigReadService, McpServers};

use crate::{FsReadService, Infrastructure};

pub struct ForgeMcpReadService<I> {
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeMcpReadService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    async fn read_config(&self, path: &Path) -> anyhow::Result<McpServers> {
        let config = self.infra.file_read_service().read_utf8(path).await?;
        Ok(serde_json::from_str(&config)?)
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> McpConfigReadService for ForgeMcpReadService<I> {
    async fn read(&self) -> anyhow::Result<McpServers> {
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
}
