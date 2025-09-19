use std::sync::Arc;

use forge_app::WorkspaceConfigService;
use forge_app::dto::WorkspaceConfig;

use crate::WorkspaceConfigRepository;

pub struct ForgeWorkspaceConfigService<I> {
    infra: Arc<I>,
}

impl<I: WorkspaceConfigRepository> ForgeWorkspaceConfigService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<I: WorkspaceConfigRepository> WorkspaceConfigService for ForgeWorkspaceConfigService<I> {
    async fn get_workspace_config(&self) -> anyhow::Result<Option<WorkspaceConfig>> {
        self.infra.get_workspace_config().await
    }

    async fn upsert_workspace_config(&self, config: WorkspaceConfig) -> anyhow::Result<()> {
        self.infra.upsert_workspace_config(config).await
    }
}
