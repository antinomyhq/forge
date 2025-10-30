use std::path::Path;
use std::sync::Arc;

use forge_app::{FileInfoInfra, FileReaderInfra, FsUndoOutput, FsUndoService};
use forge_domain::SnapshotRepository;

use crate::utils::assert_absolute_path;

/// Reverts the most recent file operation (create/modify/delete) on a specific
/// file. Use this tool when you need to recover from incorrect file changes or
/// if a revert is requested by the user.
#[derive(Default)]
pub struct ForgeFsUndo<F, R> {
    infra: Arc<F>,
    repo: Arc<R>,
}

impl<F, R> ForgeFsUndo<F, R> {
    pub fn new(infra: Arc<F>, repo: Arc<R>) -> Self {
        Self { infra, repo }
    }
}

#[async_trait::async_trait]
impl<F: FileInfoInfra + FileReaderInfra, R: SnapshotRepository> FsUndoService
    for ForgeFsUndo<F, R>
{
    async fn undo(&self, path: String) -> anyhow::Result<FsUndoOutput> {
        let mut output = FsUndoOutput::default();
        let path = Path::new(&path);
        assert_absolute_path(path)?;
        if self.infra.exists(path).await? {
            output.before_undo = Some(self.infra.read_utf8(path).await?);
        }
        self.repo.undo_snapshot(path).await?;
        if self.infra.exists(path).await? {
            output.after_undo = Some(self.infra.read_utf8(path).await?);
        }

        Ok(output)
    }
}
