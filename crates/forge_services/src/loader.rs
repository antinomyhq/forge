use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{LoaderService, Workflow};
use merge::Merge;

use crate::{FsReadService, Infrastructure};
/// Represents the possible sources of a workflow configuration
enum WorkflowSource<'a> {
    /// Explicitly provided path
    ExplicitPath(&'a Path),
    /// Default configuration embedded in the binary
    Default,
    /// Project-specific configuration in the current directory
    ProjectConfig,
}

/// A workflow loader to load the workflow from the given path.
/// It also resolves the internal paths specified in the workflow.
#[derive(Clone)]
pub struct ForgeLoaderService<F> {
    /// The application instance
    app: Arc<F>,
    path: Option<PathBuf>,
}

impl<F> ForgeLoaderService<F> {
    pub fn new(app: Arc<F>, path: Option<PathBuf>) -> Self {
        Self { app, path }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> LoaderService for ForgeLoaderService<F> {
    async fn load(&self) -> anyhow::Result<Workflow> {
        // Determine the workflow source
        let source = match &self.path {
            Some(path) => WorkflowSource::ExplicitPath(path),
            None if Path::new("forge.yaml").exists() => WorkflowSource::ProjectConfig,
            None => WorkflowSource::Default,
        };

        // Load the workflow based on its source
        match source {
            WorkflowSource::ExplicitPath(path) => self.load_from_explicit_path(path).await,
            WorkflowSource::Default => Ok(Workflow::default()),
            WorkflowSource::ProjectConfig => self.load_with_project_config().await,
        }
    }
}

impl<F: Infrastructure> ForgeLoaderService<F> {
    /// Loads a workflow from a specific file path
    async fn load_from_explicit_path(&self, path: &Path) -> anyhow::Result<Workflow> {
        let content = String::from_utf8(self.app.file_read_service().read(path).await?.to_vec())?;
        let workflow: Workflow = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse workflow from {}", path.display()))?;
        Ok(workflow)
    }

    /// Loads workflow by merging project config with default workflow
    async fn load_with_project_config(&self) -> anyhow::Result<Workflow> {
        let project_path = Path::new("forge.yaml").canonicalize()?;

        let project_content = String::from_utf8(
            self.app
                .file_read_service()
                .read(project_path.as_path())
                .await?
                .to_vec(),
        )?;

        let project_workflow: Workflow =
            serde_yaml::from_str(&project_content).with_context(|| {
                format!(
                    "Failed to parse project workflow: {}",
                    project_path.display()
                )
            })?;
        // Merge workflows with project taking precedence
        let mut merged_workflow = Workflow::default();
        merged_workflow.merge(project_workflow);
        Ok(merged_workflow)
    }
}
