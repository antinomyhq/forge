use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{Workflow, WorkflowService};

use crate::{FsReadService, FsWriteService, Infrastructure};

/// A workflow loader to load the workflow from the given path.
/// It also resolves the internal paths specified in the workflow.
pub struct ForgeWorkflowService<F> {
    infra: Arc<F>,
}

impl<F> ForgeWorkflowService<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: Infrastructure> ForgeWorkflowService<F> {
    /// Loads the workflow from the given path.
    /// If the file doesn't exist, creates a new empty workflow file at the
    /// specified path.
    pub async fn read(&self, path: &Path) -> anyhow::Result<Workflow> {
        if !path.exists() {
            let workflow = Workflow::new();
            self.infra
                .file_write_service()
                .write(path, serde_yml::to_string(&workflow)?.into())
                .await?;

            Ok(workflow)
        } else {
            let content = self.infra.file_read_service().read(path).await?;
            let workflow: Workflow = serde_yml::from_str(&content)
                .with_context(|| format!("Failed to parse workflow from {}", path.display()))?;
            Ok(workflow)
        }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> WorkflowService for ForgeWorkflowService<F> {
    async fn read(&self, path: &Path) -> anyhow::Result<Workflow> {
        self.read(path).await
    }

    async fn write(&self, path: &Path, workflow: &Workflow) -> anyhow::Result<()> {
        let content = serde_yml::to_string(workflow)?;
        self.infra
            .file_write_service()
            .write(path, content.into())
            .await
    }

    async fn update_workflow<Func>(&self, path: &Path, f: Func) -> anyhow::Result<Workflow>
    where
        Func: FnOnce(&mut Workflow) + Send,
    {
        // Read the current workflow
        let mut workflow = self.read(path).await?;

        // Apply the closure to update the workflow
        f(&mut workflow);

        // Write the updated workflow back
        self.write(path, &workflow).await?;

        Ok(workflow)
    }
}
