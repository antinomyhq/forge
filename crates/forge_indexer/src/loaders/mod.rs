use std::path::PathBuf;

pub mod file_loader;

#[derive(Debug, Clone)]
pub struct Node {
    /// File path for the chunk
    pub path: PathBuf,
    /// The content of the chunk
    pub chunk: String,
    /// The original size of the chunk
    pub original_size: usize,
}

pub trait Loader {
    async fn load(&self) -> anyhow::Result<Vec<Node>>;
}