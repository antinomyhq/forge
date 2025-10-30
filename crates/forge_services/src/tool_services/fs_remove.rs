use std::path::Path;
use std::sync::Arc;

use forge_app::{FileReaderInfra, FileRemoverInfra, FsRemoveOutput, FsRemoveService};
use forge_domain::SnapshotRepository;

use crate::utils::assert_absolute_path;

/// Service for removing files with snapshot coordination
///
/// This service coordinates between infrastructure (file I/O) and repository
/// (snapshots) to remove files while preserving the ability to undo the
/// deletion.
pub struct ForgeFsRemove<F, R> {
    infra: Arc<F>,
    repo: Arc<R>,
}

impl<F, R> ForgeFsRemove<F, R> {
    pub fn new(infra: Arc<F>, repo: Arc<R>) -> Self {
        Self { infra, repo }
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileRemoverInfra, R: SnapshotRepository> FsRemoveService
    for ForgeFsRemove<F, R>
{
    async fn remove(&self, input_path: String) -> anyhow::Result<FsRemoveOutput> {
        let path = Path::new(&input_path);
        assert_absolute_path(path)?;

        let content = self.infra.read_utf8(path).await.unwrap_or_default();

        // SNAPSHOT COORDINATION: Always capture snapshot before removing
        self.repo.insert_snapshot(path).await?;

        self.infra.remove(path).await?;

        Ok(FsRemoveOutput { content })
    }
}
