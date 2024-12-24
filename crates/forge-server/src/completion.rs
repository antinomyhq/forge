use std::path::PathBuf;

use forge_walker::Walker;
use serde::Serialize;

#[derive(Serialize)]
pub struct File {
    pub path: String,
    pub is_dir: bool,
}

pub struct Completion {
    path: String,
}

impl Completion {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    pub async fn list(&self) -> Vec<File> {
        let cwd = PathBuf::from(self.path.clone()); // Use the current working directory
        let walker = Walker::new(cwd);

        match walker.get().await {
            Ok(files) => files
                .into_iter()
                .map(|file| File { path: file.path, is_dir: file.is_dir })
                .collect(),
            Err(_) => Vec::new(), // Return an empty vector if there's an error
        }
    }
}
