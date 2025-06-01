mod tree_sitter_chunker;

pub use tree_sitter_chunker::*;

use crate::FileLoad;

/// Chunker trait for splitting documents into chunks
#[async_trait::async_trait]
pub trait Chunker: Send + Sync {
    async fn chunk(&self, input: Vec<FileLoad>) -> anyhow::Result<Vec<forge_treesitter::Block>>;
}
