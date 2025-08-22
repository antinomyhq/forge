use std::path::PathBuf;
use std::pin::Pin;

use async_stream::stream;
use derive_setters::Setters;
use futures::Stream;
use ignore::WalkBuilder;
use tokio::fs;

use crate::{Document, Loader};

/// Configuration for file loading
#[derive(Debug, Clone, Setters)]
#[setters(strip_option, into)]
pub struct FileConfig {
    /// Root path to start loading from
    pub root_path: PathBuf,
    /// File extensions to include (empty means all files)
    pub extensions: Vec<String>,
    /// Whether to use standard ignore filters (like .gitignore, node_modules,
    /// target, etc.)
    pub use_standard_filters: bool,
}

impl FileConfig {
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
            extensions: Vec::new(),
            use_standard_filters: true,
        }
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

    fn should_include_file(&self, path: &std::path::Path) -> bool {
        // Skip directories
        if path.is_dir() {
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
            let walk = WalkBuilder::new(&loader.config.root_path)
                .standard_filters(loader.config.use_standard_filters)
                .build();

            for entry in walk.flatten() {
                let path = entry.path();

                if loader.should_include_file(path) {
                    match loader.load_file(path.to_path_buf()).await {
                        Ok(document) => yield Ok(document),
                        Err(e) => yield Err(e),
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn test_file_loading_with_extensions() {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Create test files
        fs::write(temp_path.join("test.rs"), "fn main() {}").unwrap();
        fs::write(temp_path.join("test.txt"), "hello world").unwrap();
        fs::write(temp_path.join("test.md"), "# Title").unwrap();

        let config = FileConfig::new(temp_path).extensions(vec!["rs".to_string()]);
        let loader = FileLoader::new(config);

        let mut documents = Vec::new();
        let mut stream = loader.load();

        use futures::StreamExt;
        while let Some(result) = stream.next().await {
            documents.push(result.unwrap());
        }

        // Should only include .rs file
        assert_eq!(documents.len(), 1);
        assert!(documents[0].path.to_string_lossy().ends_with("test.rs"));
    }
}
