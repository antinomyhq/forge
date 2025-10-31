use std::path::Path;
use std::sync::Arc;

use forge_app::{FileInfoInfra, FileReaderInfra, FsUndoOutput, FsUndoService};
use forge_domain::SnapshotRepository;

use crate::utils::assert_absolute_path;

/// Reverts the most recent file operation (create/modify/delete) on a specific
/// file. Use this tool when you need to recover from incorrect file changes or
/// if a revert is requested by the user.
#[derive(Default)]
pub struct ForgeFsUndo<F> {
    infra: Arc<F>,
}

impl<F> ForgeFsUndo<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<F: FileInfoInfra + FileReaderInfra + SnapshotRepository> FsUndoService for ForgeFsUndo<F> {
    async fn undo(&self, path: String) -> anyhow::Result<FsUndoOutput> {
        let mut output = FsUndoOutput::default();
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        if self.infra.exists(path).await? {
            output.before_undo = Some(self.infra.read_utf8(path).await?);
        }
        self.infra.undo_snapshot(path).await?;
        if self.infra.exists(path).await? {
            output.after_undo = Some(self.infra.read_utf8(path).await?);
        }

        Ok(output)
    }
}
