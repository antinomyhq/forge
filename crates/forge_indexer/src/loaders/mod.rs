use std::path::PathBuf;

use futures::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use text_splitter::{ChunkCapacity, ChunkConfig, ChunkSizer};

use crate::loaders::file_loader::FileLoader;

pub mod file_loader;

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

fn hash(content: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(content.as_bytes());
    hasher.finalize().to_hex().to_string()
}

struct LineChunker;

impl ChunkSizer for LineChunker {
    fn size(&self, chunk: &str) -> usize {
        chunk.lines().count()
    }
}

pub async fn indexer(path: PathBuf) -> anyhow::Result<Vec<Chunk>> {
    struct FileRead {
        path: PathBuf,
        content: String,
    }

    let config = ChunkConfig::new(ChunkCapacity::new(20).with_max(100).unwrap())
        .with_sizer(LineChunker)
        .with_overlap(10)
        .unwrap();
    let splitter = text_splitter::CodeSplitter::new(tree_sitter_rust::LANGUAGE, config)?;

    // 1. Loader
    let output = FileLoader::new(path, vec!["rs".into()])
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
        })
        .collect::<Vec<_>>()
        .await;
    Ok(output)
}
