use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use forge_app::{EnvironmentInfra, Walker, WalkerInfra};
use tracing::{info, warn};

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
impl<F: WalkerInfra + EnvironmentInfra + 'static> FileDiscovery for FdWalker<F> {
    async fn discover(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let max_files = self.infra.get_environment().max_workspace_files;

        // Use the workspace file limit instead of unlimited to prevent
        // excessive memory usage when the workspace covers a very large
        // directory tree (e.g. a user's home directory).
        let walker_config = Walker::unlimited()
            .cwd(dir_path.to_path_buf())
            .skip_binary(true)
            .max_files(max_files);

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

        if paths.len() >= max_files {
            warn!(
                max_files = max_files,
                path = %dir_path.display(),
                "File discovery reached the maximum file limit; results are truncated. \
                 Set FORGE_MAX_WORKSPACE_FILES to increase the limit."
            );
        }

        info!(file_count = paths.len(), "Discovered files via walker");
        filter_and_resolve(dir_path, paths)
    }
}
