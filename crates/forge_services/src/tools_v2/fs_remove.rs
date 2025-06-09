use std::path::Path;
use std::sync::Arc;

use forge_app::{EnvironmentService, FsRemoveOutput, FsRemoveService};

use crate::utils::{assert_absolute_path, format_display_path};
use crate::{FileRemoveService, FsMetaService, Infrastructure};

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
pub struct ForgeFsRemove<T>(Arc<T>);

impl<T: Infrastructure> ForgeFsRemove<T> {
    pub fn new(infra: Arc<T>) -> Self {
        Self(infra)
    }
    /// Formats a path for display, converting absolute paths to relative when
    /// possible
    ///
    /// If the path starts with the current working directory, returns a
    /// relative path. Otherwise, returns the original absolute path.
    fn format_display_path(&self, path: &Path) -> anyhow::Result<String> {
        // Get the current working directory
        let env = self.0.environment_service().get_environment();
        let cwd = env.cwd.as_path();

        // Use the shared utility function
        format_display_path(path, cwd)
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsRemoveService for ForgeFsRemove<F> {
    async fn remove(&self, input_path: String) -> anyhow::Result<FsRemoveOutput> {
        let path = Path::new(&input_path);
        assert_absolute_path(path)?;
        // Check if the file exists
        if !self.0.file_meta_service().exists(path).await? {
            let display_path = self.format_display_path(path)?;
            return Err(anyhow::anyhow!("File not found: {}", display_path));
        }

        // Check if it's a file
        if !self.0.file_meta_service().is_file(path).await? {
            let display_path = self.format_display_path(path)?;
            return Err(anyhow::anyhow!("Path is not a file: {}", display_path));
        }

        self.0.file_remove_service().remove(path).await?;

        Ok(FsRemoveOutput { completed: true })
    }
}
