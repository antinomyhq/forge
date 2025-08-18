use std::path::Path;
use std::sync::Arc;

use forge_app::{FsRemoveOutput, FsRemoveService};

use crate::utils::assert_absolute_path;
use crate::{FileReaderInfra, FileRemoverInfra};

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
pub struct ForgeFsRemove<T>(Arc<T>);

impl<T> ForgeFsRemove<T> {
    pub fn new(infra: Arc<T>) -> Self {
        Self(infra)
    }
}

#[async_trait::async_trait]
impl<F: FileReaderInfra + FileRemoverInfra> FsRemoveService for ForgeFsRemove<F> {
    async fn remove(&self, input_path: String) -> anyhow::Result<FsRemoveOutput> {
        let path = Path::new(&input_path);
        assert_absolute_path(path)?;

        let content = self.0.read_utf8(path).await.unwrap_or_default();

        self.0.remove(path).await?;

        Ok(FsRemoveOutput { content })
    }
}
