use std::path::Path;
use std::sync::Arc;

use forge_app::{FsRemoveOutput, FsRemoveService};

use crate::utils::assert_absolute_path;
use crate::{FileRemoveService, FsMetaService, Infrastructure};

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
pub struct ForgeFsRemove<T>(Arc<T>);

impl<T: Infrastructure> ForgeFsRemove<T> {
    pub fn new(infra: Arc<T>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> FsRemoveService for ForgeFsRemove<F> {
    async fn remove(&self, input_path: String) -> anyhow::Result<FsRemoveOutput> {
        let path = Path::new(&input_path);
        assert_absolute_path(path)?;
        // Check if the file exists
        if !self.0.file_meta_service().exists(path).await? {
            return Ok(FsRemoveOutput::FileNotFound);
        }

        // Check if it's a file
        if !self.0.file_meta_service().is_file(path).await? {
            return Ok(FsRemoveOutput::NotAFile);
        }

        self.0.file_remove_service().remove(path).await?;

        Ok(FsRemoveOutput::Success)
    }
}
