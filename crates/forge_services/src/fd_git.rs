use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use forge_app::{CommandInfra, EnvironmentInfra};
use tracing::{info, warn};

use crate::fd::{FileDiscovery, filter_and_resolve};

/// File discovery implementation backed by `git ls-files`.
///
/// Returns the files tracked by git in the repository rooted at `dir_path`.
/// This is the preferred strategy when the workspace is a git repository with
/// at least one commit.
pub struct FsGit<F> {
    infra: Arc<F>,
}

impl<F> FsGit<F> {
    /// Creates a new `GitFileDiscovery` using the provided infrastructure.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: CommandInfra + EnvironmentInfra> FsGit<F> {
    /// Runs `git ls-files` in `dir_path` and returns the resulting file paths.
    ///
    /// # Errors
    ///
    /// Returns an error when the command cannot be executed or exits with a
    /// non-zero status (e.g. the directory is not a git repository).
    async fn git_ls_files(
        &self,
        dir_path: &Path,
        max_files: usize,
    ) -> anyhow::Result<Vec<String>> {
        let output = self
            .infra
            .execute_command(
                "git ls-files".to_string(),
                dir_path.to_path_buf(),
                true,
                None,
            )
            .await?;

        if output.exit_code != Some(0) {
            let err = anyhow::anyhow!(output.stderr);
            return Err(match output.exit_code {
                Some(code) => err.context(format!("'git ls-files' exited with code {}", code)),
                None => err,
            });
        }

        let paths: Vec<String> = output
            .stdout
            .lines()
            .filter(|line| !line.is_empty())
            .take(max_files)
            .map(|line| line.trim().to_string())
            .collect();

        Ok(paths)
    }
}

#[async_trait]
impl<F: CommandInfra + EnvironmentInfra + 'static> FileDiscovery for FsGit<F> {
    async fn discover(&self, dir_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        let max_files = self.infra.get_environment().max_workspace_files;
        let paths = self.git_ls_files(dir_path, max_files).await?;
        if paths.is_empty() {
            return Err(anyhow::anyhow!("git ls-files returned no files"));
        }
        if paths.len() >= max_files {
            warn!(
                max_files = max_files,
                path = %dir_path.display(),
                "git ls-files reached the maximum file limit; results are truncated. \
                 Set FORGE_MAX_WORKSPACE_FILES to increase the limit."
            );
        }
        info!(
            file_count = paths.len(),
            "Discovered files via git ls-files"
        );
        filter_and_resolve(dir_path, paths)
    }
}
