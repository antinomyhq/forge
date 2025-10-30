use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use forge_app::{
    FileDirectoryInfra, FileInfoInfra, FileReaderInfra, FileWriterInfra, FsCreateOutput,
    FsCreateService,
};
use forge_domain::SnapshotRepository;

use crate::tool_services;
use crate::utils::assert_absolute_path;

/// Service for creating files with snapshot coordination
///
/// This service coordinates between infrastructure (file I/O) and repository
/// (snapshots) to create files while preserving the ability to undo changes.
pub struct ForgeFsCreate<F, R> {
    infra: Arc<F>,
    repo: Arc<R>,
}

impl<F, R> ForgeFsCreate<F, R> {
    pub fn new(infra: Arc<F>, repo: Arc<R>) -> Self {
        Self { infra, repo }
    }
}

#[async_trait::async_trait]
impl<
    F: FileDirectoryInfra + FileInfoInfra + FileReaderInfra + FileWriterInfra + Send + Sync,
    R: SnapshotRepository,
> FsCreateService for ForgeFsCreate<F, R>
{
    async fn create(
        &self,
        path: String,
        content: String,
        overwrite: bool,
        capture_snapshot: bool,
    ) -> anyhow::Result<FsCreateOutput> {
        let path = Path::new(&path);
        assert_absolute_path(path)?;

        // Validate file content if it's a supported language file
        let syntax_warning = tool_services::syn::validate(path, &content);

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

        // SNAPSHOT COORDINATION: Capture snapshot before writing if requested and file
        // exists
        if file_exists && capture_snapshot {
            self.repo.insert_snapshot(path).await?;
        }

        // Write file only after validation passes and directories are created
        self.infra
            .write(path, Bytes::from(content), false) // Pass false since we handle snapshots above
            .await?;

        Ok(FsCreateOutput {
            path: path.display().to_string(),
            before: old_content,
            warning: syntax_warning.map(|v| v.to_string()),
        })
    }
}
