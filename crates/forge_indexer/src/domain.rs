use std::path::PathBuf;
use std::time::SystemTime;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// A document loaded from a source
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Document {
    pub path: PathBuf,
    pub content: String,
}

impl Document {
    pub fn new(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        let path = path.into();
        let content = content.into();

        Self { path, content }
    }
}

/// A chunk of a document
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Chunk {
    pub content: String,
    pub position: Position,
}

impl Chunk {
    pub fn new(content: impl Into<String>, position: Position) -> Self {
        let content = content.into();
        Self { content, position }
    }
}

/// Position information for a chunk within its source document
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Position {
    pub start_char: usize,
    pub end_char: usize,
}

impl Position {
    pub fn new(start_char: usize, end_char: usize) -> Self {
        Self { start_char, end_char }
    }
}

/// A chunk with its embedding
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct EmbeddedChunk {
    pub chunk: Chunk,
    pub embedding: Vec<f32>,
    pub embedding_model: String,
}

impl EmbeddedChunk {
    pub fn new(chunk: Chunk, embedding: Vec<f32>, embedding_model: impl Into<String>) -> Self {
        Self { chunk, embedding, embedding_model: embedding_model.into() }
    }
}

/// A chunk that has been stored
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct StoredChunk {
    pub embedded_chunk: EmbeddedChunk,
    pub storage_id: String,
    pub stored_at: SystemTime,
}

impl StoredChunk {
    pub fn new(embedded_chunk: EmbeddedChunk, storage_id: impl Into<String>) -> Self {
        Self {
            embedded_chunk,
            storage_id: storage_id.into(),
            stored_at: SystemTime::now(),
        }
    }
}
