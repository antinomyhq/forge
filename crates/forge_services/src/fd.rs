use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use forge_app::{CommandInfra, WalkerInfra};
use forge_domain::WorkspaceId;
use tracing::{info, warn};

use crate::error::Error as ServiceError;
use crate::fd_git::FsGit;
use crate::fd_walker::FdWalker;

pub(crate) static ALLOWED_EXTENSIONS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let extensions_str = include_str!("allowed_extensions.txt");
    extensions_str
        .lines()
        .map(|line| line.trim().to_lowercase())
        .filter(|line| !line.is_empty())
        .collect()
});

/// Returns `true` if `path` carries an extension present in the allowed
/// extensions list.
pub(crate) fn has_allowed_extension(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ALLOWED_EXTENSIONS.contains(&ext.to_string_lossy().to_lowercase() as &str)
    } else {
        false
    }
}

/// Filters relative path strings down to those with an allowed extension,
/// resolves each against `dir_path`, and returns them as absolute `PathBuf`s.
///
/// Returns an error when the filtered list is empty, indicating no indexable
/// source files exist in the workspace.
pub(crate) fn filter_and_resolve(
    dir_path: &Path,
    paths: impl IntoIterator<Item = String>,
) -> anyhow::Result<Vec<PathBuf>> {
    let filtered: Vec<PathBuf> = paths
        .into_iter()
        .map(|p| dir_path.join(&p))
        .filter(|p| has_allowed_extension(p))
        .collect();

    if filtered.is_empty() {
        return Err(ServiceError::NoSourceFilesFound.into());
    }

    Ok(filtered)
}

/// Trait for discovering the list of files in a workspace directory that
/// should be considered for synchronisation.
///
/// Implementations may use different strategies (e.g. `git ls-files` or a
/// plain filesystem walk) to enumerate files. The returned paths are absolute.
#[async_trait]
pub trait FileDiscovery: Send + Sync {
    /// Returns the absolute paths of all files to be indexed under `dir_path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the discovery strategy fails and no files can be
    /// enumerated.
    async fn discover(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>>;
}

/// Discovers workspace files using a `FileDiscovery` implementation and logs
/// progress associated with `workspace_id`.
pub async fn discover_sync_file_paths(
    discovery: &impl FileDiscovery,
    dir_path: &Path,
    workspace_id: &WorkspaceId,
) -> anyhow::Result<Vec<PathBuf>> {
    info!(workspace_id = %workspace_id, "Discovering files for sync");
    let files = discovery.discover(dir_path).await?;
    info!(
        workspace_id = %workspace_id,
        count = files.len(),
        "Files discovered and filtered for sync"
    );
    Ok(files)
}

/// A `FileDiscovery` implementation that routes between `GitFileDiscovery` and
/// `WalkerFileDiscovery`.
///
/// It first attempts git-based discovery. If git is unavailable, returns no
/// files, or fails for any reason it transparently falls back to the filesystem
/// walker so that workspaces without git history are still indexed correctly.
pub struct FdDefault<F> {
    git: FsGit<F>,
    walker: FdWalker<F>,
}

impl<F> FdDefault<F> {
    /// Creates a new `RoutingFileDiscovery` using the provided infrastructure
    /// for both the git and walker strategies.
    pub fn new(infra: Arc<F>) -> Self {
        Self { git: FsGit::new(infra.clone()), walker: FdWalker::new(infra) }
    }
}

#[async_trait]
impl<F: CommandInfra + WalkerInfra + 'static> FileDiscovery for FdDefault<F> {
    async fn discover(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        match self.git.discover(dir_path).await {
            Ok(files) => Ok(files),
            Err(err) => {
                warn!(error = ?err, "git-based file discovery failed, falling back to walker");
                self.walker.discover(dir_path).await
            }
        }
    }
}
