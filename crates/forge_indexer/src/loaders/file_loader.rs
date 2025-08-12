use std::path::PathBuf;

use derive_setters::Setters;

use crate::loaders::Loader;

#[derive(Debug, Clone, Setters)]
pub struct FileLoader {
    pub(crate) path: PathBuf,
    pub(crate) ext: Option<Vec<String>>,
}

impl FileLoader {
    pub fn new<P: AsRef<PathBuf>>(path: P) -> Self {
        Self { path: path.as_ref().to_path_buf(), ext: None }
    }
}

impl Loader for FileLoader {
    async fn load(&self) -> anyhow::Result<Vec<super::Node>> {
        let walker = forge_walker::Walker::max_all()
            .cwd(self.path.clone())
            .skip_binary(true);

        // Get the list of files
        let mut files = walker
            .get()
            .await?;

        // Filter by extension
        if let Some(ext) = &self.ext {
            files.retain(|node| {
                if let Some(file_ext) = PathBuf::from(&node.path).extension() {
                    return ext.contains(&file_ext.to_string_lossy().to_string());
                }
                false
            });
        }

        // Read file contents
        let mut nodes = Vec::with_capacity(files.len());
        for node in files {
            let content = std::fs::read_to_string(&node.path)?;
            nodes.push(super::Node {
                path: PathBuf::from(&node.path),
                original_size: content.len(),
                chunk: content,
            });
        }

        Ok(nodes)
    }
}
