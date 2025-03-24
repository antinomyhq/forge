use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use forge_services::{FsSnapshotService, FsWriteService};

pub struct ForgeFileWriteService<S> {
    snaps: Arc<S>,
}

impl<S> ForgeFileWriteService<S> {
    pub fn new(snaps: Arc<S>) -> Self {
        Self { snaps }
    }
}

#[async_trait::async_trait]
impl<S: FsSnapshotService> FsWriteService for ForgeFileWriteService<S> {
    async fn write(&self, path: &Path, contents: Bytes) -> Result<()> {
        if forge_fs::ForgeFS::exists(path) {
            let _ = self.snaps.create_snapshot(path).await?;
        }

        Ok(forge_fs::ForgeFS::write(path, contents.to_vec()).await?)
    }
}
