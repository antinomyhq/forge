use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_app::{FileReadService, Infrastructure};
use forge_domain::Workflow;

// Default forge.yaml content embedded in the binary
const DEFAULT_FORGE_WORKFLOW: &str = include_str!("../../../forge.default.yaml");

/// A workflow loader to load the workflow from the given path.
/// It also resolves the internal paths specified in the workflow.
pub struct ForgeLoaderService<F>(Arc<F>);

impl<F> ForgeLoaderService<F> {
    pub fn new(app: Arc<F>) -> Self {
        Self(app)
    }
}

impl<F: Infrastructure> ForgeLoaderService<F> {
    /// loads the workflow from the given path.
    /// Loads the workflow from the given path if provided, otherwise tries to
    /// read from current directory's forge.yaml, and falls back to embedded
    /// default if neither exists.
    pub async fn load(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let content = match path {
            Some(path) => String::from_utf8(self.0.file_read_service().read(path).await?.to_vec())?,
            None => {
                let current_dir_config = Path::new("forge.yaml");
                if current_dir_config.exists() {
                    String::from_utf8(
                        self.0
                            .file_read_service()
                            .read(current_dir_config)
                            .await?
                            .to_vec(),
                    )?
                } else {
                    DEFAULT_FORGE_WORKFLOW.to_string()
                }
            }
        };

        let workflow: Workflow =
            serde_yaml::from_str(&content).with_context(|| "Failed to parse workflow")?;
        Ok(workflow)
    }
}
