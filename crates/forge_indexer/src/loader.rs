use std::path::PathBuf;
use std::pin::Pin;

use async_stream::stream;
use derive_setters::Setters;
use futures::Stream;
use tokio::fs;
use walkdir::WalkDir;

use crate::{Document, Loader};

/// Configuration for file loading
#[derive(Debug, Clone, Setters)]
#[setters(strip_option, into)]
pub struct FileConfig {
    /// Root path to start loading from
    pub root_path: PathBuf,
    /// File extensions to include (empty means all files)
    pub extensions: Vec<String>,
    /// Patterns to ignore (directories or files containing these patterns)
    pub ignore_patterns: Vec<String>,
    /// Maximum file size in bytes (None means no limit)
    pub max_file_size: Option<usize>,
}

impl FileConfig {
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
            extensions: Vec::new(),
            ignore_patterns: Vec::new(),
            max_file_size: None,
        }
    }

    pub fn test() -> Self {
        Self::new("./test")
            .extensions(vec!["rs".to_string(), "md".to_string()])
            .ignore_patterns(vec!["target".to_string(), ".git".to_string()])
            .max_file_size(1024_usize * 1024) // 1MB
    }
}

impl Default for FileConfig {
    fn default() -> Self {
        Self::test()
    }
}

/// Loader implementation for files
#[derive(Debug, Clone)]
pub struct FileLoader {
    config: FileConfig,
}

impl FileLoader {
    pub fn new(config: FileConfig) -> Self {
        Self { config }
    }

    pub fn test() -> Self {
        Self::new(FileConfig::test())
    }

    fn should_include_file(&self, entry: &walkdir::DirEntry) -> bool {
        let path = entry.path();

        // Skip directories
        if !entry.file_type().is_file() {
            return false;
        }

        // Check file size if limit is set
        if let Some(max_size) = self.config.max_file_size
            && let Ok(metadata) = entry.metadata()
            && metadata.len() > max_size as u64
        {
            return false;
        }

        // Check extensions if specified
        if !self.config.extensions.is_empty() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if !self.config.extensions.contains(&ext.to_string()) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check ignore patterns
        let path_str = path.to_string_lossy();
        if self
            .config
            .ignore_patterns
            .iter()
            .any(|pattern| path_str.contains(pattern))
        {
            return false;
        }

        true
    }

    async fn load_file(&self, path: PathBuf) -> anyhow::Result<Document> {
        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", path.display(), e))?;
        Ok(Document { path, content })
    }
}

impl Loader for FileLoader {
    type Item = Document;
    type Stream = Pin<Box<dyn Stream<Item = anyhow::Result<Document>> + Send>>;

    fn load(&self) -> Self::Stream {
        let loader = self.clone();

        Box::pin(stream! {
            let walker = WalkDir::new(&loader.config.root_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| loader.should_include_file(entry));

            for entry in walker {
                let path = entry.path().to_path_buf();
                match loader.load_file(path).await {
                    Ok(document) => yield Ok(document),
                    Err(e) => yield Err(e),
                }
            }
        })
    }
}
