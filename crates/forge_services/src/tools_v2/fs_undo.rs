use std::path::Path;
use std::sync::Arc;

use forge_app::{EnvironmentService, FsUndoOutput, FsUndoService};

use crate::utils::{assert_absolute_path, format_display_path};
use crate::{FsSnapshotService, Infrastructure};

/// Reverts the most recent file operation (create/modify/delete) on a specific
/// file. Use this tool when you need to recover from incorrect file changes or
/// if a revert is requested by the user.
#[derive(Default)]
pub struct ForgeFsUndo<F>(Arc<F>);

impl<F: Infrastructure> ForgeFsUndo<F> {
    pub fn new(infra: Arc<F>) -> Self {
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
impl<F: Infrastructure> FsUndoService for ForgeFsUndo<F> {
    async fn undo(&self, path: String) -> anyhow::Result<FsUndoOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        self.0.file_snapshot_service().undo_snapshot(path).await?;
        // Format the path for display
        let display_path = self.format_display_path(path)?;
        Ok(FsUndoOutput::from(display_path))
    }
}
