use std::path::{Path, PathBuf};
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

    async fn write_temp(&self, prefix: &str, ext: &str, content: &str) -> anyhow::Result<PathBuf> {
        let path = tempfile::Builder::new()
            .keep(true)
            .prefix(prefix)
            .suffix(ext)
            .tempfile()?
            .into_temp_path()
            .to_path_buf();

        self.write(&path, content.to_string().into()).await?;

        Ok(path)
    }
}
