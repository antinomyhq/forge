use std::path::PathBuf;

use ignore::WalkBuilder;
use tokio::task::spawn_blocking;

use crate::{Error, Result};

pub struct File {
    pub path: String,
    pub is_dir: bool,
}

pub struct Walker {
    cwd: PathBuf,
}

impl Walker {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd: cwd.clone() }
    }

    pub async fn get(&self) -> Result<Vec<File>> {
        let cwd = self.cwd.clone();
        match spawn_blocking(move || Self::get_blocking(cwd)).await {
            Ok(result) => result,
            Err(e) => Err(Error::JoinError(e)),
        }
    }

    /// Internal function to scan filesystem
    fn get_blocking(cwd: PathBuf) -> Result<Vec<File>> {
        let mut files = Vec::new();
        let walk = WalkBuilder::new(cwd.clone())
            .hidden(true) // Skip hidden files
            .git_global(true) // Use global gitignore
            .git_ignore(true) // Use local .gitignore
            .ignore(true) // Use .ignore files
            .build();

        for entry in walk.flatten() {
            let path = entry.path();
            let relative_path = path
                .strip_prefix(&cwd)
                .map_err(|_| Error::InvalidPath(path.to_string_lossy().to_string()))?;
            let path_string = relative_path.to_string_lossy().to_string();

            files.push(File { path: path_string, is_dir: path.is_dir() });
        }

        Ok(files)
    }
}
