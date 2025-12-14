use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_app::{
    FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra, FsCreateOutput,
    FsCreateService, compute_hash,
};
use forge_domain::{SnapshotRepository, ValidationRepository};

use crate::utils::assert_absolute_path;

/// Service for creating files with snapshot coordination
///
/// This service coordinates between infrastructure (file I/O) and repository
/// (snapshots) to create files while preserving the ability to undo changes.
pub struct ForgeFsCreate<F> {
    infra: Arc<F>,
}

impl<F> ForgeFsCreate<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<
    F: FileDirectoryInfra
        + FileInfoInfra
        + FileReaderInfra
        + FileWriterInfra
        + SnapshotRepository
        + ValidationRepository
        + Send
        + Sync,
> FsCreateService for ForgeFsCreate<F>
{
    async fn create(
        &self,
        path: String,
        content: String,
        overwrite: bool,
    ) -> anyhow::Result<FsCreateOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;

        // Validate file syntax using remote validation API (graceful failure)
        let syntax_warning = self
            .infra
            .validate_file(path, &content)
            .await
            .ok()
            .flatten();

        if let Some(parent) = Path::new(&path).parent() {
            self.infra
                .create_dirs(parent)
                .await
                .with_context(|| format!("Failed to create directories: {}", path.display()))?;
        }

        // Check if the file exists
        let file_exists = self.infra.is_file(path).await?;

        // If file exists and overwrite flag is not set, return an error
        if file_exists && !overwrite {
            return Err(anyhow::anyhow!(
                "Cannot overwrite existing file: overwrite flag not set.",
            ))
            .with_context(|| format!("File already exists at {}", path.display()));
        }

        // Record the file content before modification
        let old_content = if file_exists && overwrite {
            Some(self.infra.read_utf8(path).await?)
        } else {
            None
        };

        // SNAPSHOT COORDINATION: Capture snapshot before writing if file exists
        if file_exists {
            self.infra.insert_snapshot(path).await?;
        }

        // Write file only after validation passes and directories are created
        self.infra.write(path, Bytes::from(content.clone())).await?;

        // Compute hash of the written file content
        let content_hash = compute_hash(&content);

        Ok(FsCreateOutput {
            path: path.display().to_string(),
            before: old_content,
            warning: syntax_warning,
            content_hash,
        })
    }
}
