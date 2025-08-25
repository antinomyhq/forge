use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use derive_setters::Setters;
use ignore::{WalkBuilder, WalkState};
use tokio::fs;

use crate::Document;
use crate::transform::Transform;

/// Loader implementation for files
#[derive(Debug, Clone, Default, Setters)]
pub struct FileLoader {
    extensions: Vec<String>,
}

impl FileLoader {
    fn should_include_file(&self, path: &std::path::Path) -> bool {
        // Skip directories
        if path.is_dir() {
            return false;
        }

        // Check extensions if specified
        if !self.extensions.is_empty() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if !self.extensions.contains(&ext.to_string()) {
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

impl Transform for FileLoader {
    type In = PathBuf;
    type Out = Vec<Document>;
    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let walk = WalkBuilder::new(input)
            .standard_filters(true)
            .build_parallel();

        // run the walker in parallel.
        let dents = Arc::new(Mutex::new(vec![]));
        walk.run(|| {
            let dents = dents.clone();
            Box::new(move |result| {
                if let Ok(dent) = result {
                    dents.lock().unwrap().push(dent);
                }
                WalkState::Continue
            })
        });

        let mut documents = vec![];
        for entry in dents
            .lock()
            .unwrap()
            .to_vec()
            .into_iter()
            .filter(|path| self.should_include_file(path.path()))
        {
            documents.push(self.load_file(entry.path().to_path_buf()).await?);
        }

        Ok(documents)
    }
}
