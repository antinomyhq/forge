use std::{path::PathBuf};

use futures::StreamExt;
// use futures::future::TryFutureExt;
use futures::stream::TryStreamExt;
use serde::{Deserialize, Serialize};

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
    pub pos: Position,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub start: usize,
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

async fn example() {
    struct FileRead {
        path: PathBuf,
        content: String,
    }

    // 1. Loader
    // let output = FileLoader::new("path/to/directory", vec![".rs".into()])
    //     .flat_map(|paths| tokio_stream::iter(paths.into_iter()))
    //     .chunks(128)
    //     .then(|paths| {
    //         futures::future::join_all(
    //             paths
    //                 .into_iter()
    //                 .map(|path| {
    //                     FileRead {
    //                         path: path.clone(),
    //                         content: tokio::fs::read_to_string(path),
    //                     }
    //                 }),
    //         )
    //     }).then(|chunks| {
    //         todo!()
    //     });

    // 1. Loader
    let output = FileLoader::new("path/to/directory", vec![".rs".into()])
        .flat_map(|paths| tokio_stream::iter(paths.into_iter()))
        .and_then(|path| {
            tokio::fs::read_to_string(path.clone())
                .map(|content| content.map(|c| FileRead { content: c, path }))
        })
        .buffer_unordered(128)
        .map(|file| {});
    // .then(|file| todo!());

    // 2. create code semantic chunks

    todo!()
}
