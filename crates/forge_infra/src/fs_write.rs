use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_services::{FileWriterInfra, SnapshotInfra};

pub struct ForgeFileWriteService<S> {
    snaps: Arc<S>,
}

impl<S> ForgeFileWriteService<S> {
    pub fn new(snaps: Arc<S>) -> Self {
        Self { snaps }
    }

    // To ensure the path is valid, create parent directories for the given file
    // if the file did not exist.
    async fn create_parent_dirs(&self, path: &Path) -> anyhow::Result<()> {
        if !forge_fs::ForgeFS::exists(path)
            && let Some(parent) = path.parent()
        {
            forge_fs::ForgeFS::create_dir_all(parent).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<S: SnapshotInfra> FileWriterInfra for ForgeFileWriteService<S> {
    async fn write(
        &self,
        path: &Path,
        contents: Bytes,
        capture_snapshot: bool,
    ) -> anyhow::Result<()> {
        self.create_parent_dirs(path).await?;
        if capture_snapshot {
            let _ = self.snaps.create_snapshot(path).await?;
        }

        Ok(forge_fs::ForgeFS::write(path, contents.to_vec()).await?)
    }

    async fn write_temp(&self, prefix: &str, ext: &str, content: &str) -> anyhow::Result<PathBuf> {
        let path = tempfile::Builder::new()
            .disable_cleanup(true)
            .prefix(prefix)
            .suffix(ext)
            .tempfile()?
            .into_temp_path()
            .to_path_buf();

        self.create_parent_dirs(&path).await?;
        self.write(&path, content.to_string().into(), false).await?;

        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_snaps::Snapshot;
    use tempfile::tempdir;

    use super::*;

    struct MockSnapshotService;

    #[async_trait::async_trait]
    impl SnapshotInfra for MockSnapshotService {
        async fn create_snapshot(&self, _: &Path) -> anyhow::Result<forge_snaps::Snapshot> {
            Ok(Snapshot {
                id: Default::default(),
                timestamp: Default::default(),
                path: "".to_string(),
            })
        }

        async fn undo_snapshot(&self, _path: &Path) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn create_test_service() -> ForgeFileWriteService<MockSnapshotService> {
        ForgeFileWriteService::new(Arc::new(MockSnapshotService))
    }

    #[tokio::test]
    async fn test_create_parent_dirs_when_file_does_not_exist() {
        let temp_dir = tempdir().unwrap();
        let service = create_test_service();

        let nested_file_path = temp_dir
            .path()
            .join("level1")
            .join("level2")
            .join("test.txt");

        let actual = service
            .write(
                &nested_file_path,
                Bytes::from_static("foo".as_bytes()),
                false,
            )
            .await;

        assert!(actual.is_ok());
        assert!(nested_file_path.parent().unwrap().exists());
    }
}
