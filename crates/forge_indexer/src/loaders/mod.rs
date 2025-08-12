use std::path::PathBuf;

use futures::{FutureExt, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use text_splitter::{ChunkCapacity, ChunkConfig};
use tree_sitter::Language;

use crate::loaders::file_loader::FileLoader;

pub mod file_loader;

#[derive(Debug, Clone)]
pub struct Node {
    /// File path for the chunk
    pub path: PathBuf,
    /// The content of the chunk
    pub content: String,
}

pub trait Loader {
    async fn load(&self) -> anyhow::Result<Vec<Node>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub content: String,
    pub path: PathBuf,
    pub hash: String,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub chat_offset: usize,
    pub end: usize,
}

pub trait Chunker {
    fn chunk(&self, node: &Node) -> anyhow::Result<Vec<Chunk>>;
}

// send chunks to BE for embedding.
pub trait Embedder {
    async fn embed(&self, node: &Node) -> anyhow::Result<Vec<u8>>;
    async fn batch_embed(&self, nodes: &[Node]) -> anyhow::Result<Vec<Vec<u8>>>;
}

fn hash(content: &str) -> String {
    todo!()
}

async fn example() -> anyhow::Result<()> {
    struct FileRead {
        path: PathBuf,
        content: String,
    }

    let config = ChunkConfig::new(ChunkCapacity::new(1024));
    let splitter = text_splitter::CodeSplitter::new(tree_sitter_rust::LANGUAGE, config)?;

    // 1. Loader
    let output = FileLoader::new("path/to/directory", vec![".rs".into()])
        .flat_map(|paths| tokio_stream::iter(paths.into_iter()))
        .map(|path| {
            tokio::fs::read_to_string(path.clone())
                .map(|content| content.map(|c| FileRead { content: c, path }))
        })
        .buffer_unordered(1024)
        .flat_map(|file| {
            let file = file.unwrap();
            let content = file.content;
            let content = splitter
                .chunk_char_indices(&content)
                .map(|item| Chunk {
                    path: file.path.to_owned(),
                    hash: hash(item.chunk),
                    position: Position {
                        chat_offset: item.char_offset,
                        end: item.char_offset + item.chunk.chars().count(),
                    },
                    content: item.chunk.to_owned(),
                })
                .collect::<Vec<_>>();
            tokio_stream::iter(content)
        });
    // .then(|file| todo!());

    // 2. create code semantic chunks

    todo!()
}
