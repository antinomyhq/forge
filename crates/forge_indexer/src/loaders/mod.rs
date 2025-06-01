mod file_loader;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub use file_loader::*;

#[derive(Debug, Eq, PartialEq)]
pub struct FileLoad {
    pub path: PathBuf,
    pub content: String,
}

impl Ord for FileLoad {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path.cmp(&other.path)
    }
}

impl PartialOrd for FileLoad {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Loader trait for loading documents
#[async_trait::async_trait]
pub trait Loader: Send + Sync {
    async fn load(&self, path: &Path) -> anyhow::Result<Vec<FileLoad>>;
}
