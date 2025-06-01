use std::fmt::Display;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use forge_walker::Walker;
use tracing::info;

use super::{FileLoad, Loader};

#[derive(Default)]
pub enum Extension {
    #[default]
    Rust,
}

impl Display for Extension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Extension::Rust => write!(f, "rs"),
        }
    }
}

pub struct FileLoader {
    exts: Vec<Extension>,
}

impl Default for FileLoader {
    fn default() -> Self {
        Self { exts: vec![Extension::default()] }
    }
}

impl FileLoader {
    /// Sets the file extensions to load
    pub fn with_extensions(mut self, exts: Vec<Extension>) -> Self {
        self.exts = exts;
        self
    }

    /// Loads files from the given path
    async fn load(&self, path: &Path) -> Result<Vec<FileLoad>> {
        if path.is_dir() {
            self.load_directory(path).await
        } else {
            self.load_file(&path.to_path_buf())
                .await
                .map(|out| vec![out])
        }
    }

    /// Loads files from the given directory
    async fn load_directory(&self, path: &Path) -> Result<Vec<FileLoad>> {
        let walker = Walker::max_all();
        let files = walker
            .cwd(path.to_path_buf())
            .get()
            .await
            .with_context(|| format!("Failed to walk directory: {}", path.display()))?
            .into_iter()
            .filter(|f| self.should_include_file(&PathBuf::from(&f.path)))
            .collect::<Vec<_>>();

        // TODO: read in parallel.
        let mut result = Vec::with_capacity(files.len());
        for file in files {
            let file_path = PathBuf::from(&file.path);
            result.push(self.load_file(&file_path).await?);
        }

        result.sort();
        Ok(result)
    }

    /// Returns true if the file should be included based on its extension
    fn should_include_file(&self, path: &Path) -> bool {
        path.extension()
            .map(|ext| ext.to_string_lossy().to_string())
            .is_some_and(|file_ext| self.exts.iter().any(|ext| ext.to_string() == file_ext))
    }

    /// Loads a single file and returns its content and language
    async fn load_file(&self, path: &PathBuf) -> Result<FileLoad> {
        if self.should_include_file(path) {
            let content = tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("Failed to read file: {}", path.display()))?;

            Ok(FileLoad { content, path: path.clone() })
        } else {
            Err(anyhow::anyhow!("Unsupported file format."))
        }
    }
}

#[async_trait::async_trait]
impl Loader for FileLoader {
    async fn load(&self, path: &Path) -> Result<Vec<FileLoad>> {
        info!("Loading files from: {}", path.display());
        let result = self.load(path).await?;
        info!("'{}' files loaded from {}", result.len(), path.display());
        Ok(result)
    }
}
