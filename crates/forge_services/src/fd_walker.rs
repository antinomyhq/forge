use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use forge_app::{Walker, WalkerInfra};
use tracing::info;

use crate::fd::{FileDiscovery, filter_and_resolve};

/// File discovery implementation backed by the filesystem walker.
///
/// Walks the directory tree under `dir_path` using the configured `WalkerInfra`
/// implementation. This is used as a fallback when git-based discovery is
/// unavailable or returns no files.
pub struct FdWalker<F> {
    infra: Arc<F>,
}

impl<F> FdWalker<F> {
    /// Creates a new `WalkerFileDiscovery` using the provided infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait]
impl<F: WalkerInfra + 'static> FileDiscovery for FdWalker<F> {
    async fn discover(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let walker_config = Walker::unlimited()
            .cwd(dir_path.to_path_buf())
            .skip_binary(true);

        let files = self
            .infra
            .walk(walker_config)
            .await
            .context("Failed to walk directory")?;

        let paths: Vec<String> = files
            .into_iter()
            .filter(|f| !f.is_dir())
            .map(|f| f.path)
            .collect();

        info!(file_count = paths.len(), "Discovered files via walker");
        filter_and_resolve(dir_path, paths)
    }
}
