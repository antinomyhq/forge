use std::path::{Path, PathBuf};
use std::time::SystemTime;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// A document loaded from a source
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Document {
    pub path: PathBuf,
    pub content: String,
    pub metadata: DocumentMetadata,
}

impl Document {
    pub fn new(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        let path = path.into();
        let content = content.into();
        let metadata = DocumentMetadata::new(&path, &content);

        Self { path, content, metadata }
    }

    pub fn test() -> Self {
        Self::new("test.rs", "fn main() { println!(\"Hello, world!\"); }")
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::test()
    }
}

/// Metadata about a document
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct DocumentMetadata {
    pub file_type: String,
    pub size: usize,
    pub modified: SystemTime,
    pub hash: String,
}

impl DocumentMetadata {
    pub fn new(path: &Path, content: &str) -> Self {
        Self {
            file_type: path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            size: content.len(),
            modified: SystemTime::now(),
            hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
        }
    }

    pub fn test() -> Self {
        Self::new(&PathBuf::from("test.rs"), "test content")
    }
}

impl Default for DocumentMetadata {
    fn default() -> Self {
        Self::test()
    }
}

/// A chunk of a document
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Chunk {
    pub content: String,
    pub source: ChunkSource,
    pub position: Position,
    pub metadata: ChunkMetadata,
}

impl Chunk {
    pub fn new(content: impl Into<String>, source: ChunkSource, position: Position) -> Self {
        let content = content.into();
        let metadata = ChunkMetadata::new(&content);

        Self { content, source, position, metadata }
    }

    pub fn test() -> Self {
        Self::new(
            "fn main() { println!(\"Hello, world!\"); }",
            ChunkSource::test(),
            Position::test(),
        )
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::test()
    }
}

/// Source information for a chunk
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct ChunkSource {
    pub document_path: PathBuf,
    pub document_hash: String,
}

impl ChunkSource {
    pub fn new(document_path: impl Into<PathBuf>, document_hash: impl Into<String>) -> Self {
        Self {
            document_path: document_path.into(),
            document_hash: document_hash.into(),
        }
    }

    pub fn test() -> Self {
        Self::new("test.rs", "test_hash")
    }
}

impl Default for ChunkSource {
    fn default() -> Self {
        Self::test()
    }
}

/// Position information for a chunk within its source document
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Position {
    pub start_char: usize,
    pub end_char: usize,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

impl Position {
    pub fn new(start_char: usize, end_char: usize) -> Self {
        Self { start_char, end_char, start_line: None, end_line: None }
    }

    pub fn test() -> Self {
        Self::new(0, 42)
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::test()
    }
}

/// Metadata about a chunk
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct ChunkMetadata {
    pub chunk_type: ChunkType,
    pub language: Option<String>,
    pub hash: String,
}

impl ChunkMetadata {
    pub fn new(content: &str) -> Self {
        Self {
            chunk_type: ChunkType::Code,
            language: Some("rust".to_string()),
            hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
        }
    }

    pub fn test() -> Self {
        Self::new("test content")
    }
}

impl Default for ChunkMetadata {
    fn default() -> Self {
        Self::test()
    }
}

/// Type of chunk content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkType {
    Code,
    Documentation,
    Configuration,
    Text,
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

    pub fn test() -> Self {
        Self::new(Chunk::test(), vec![0.1, 0.2, 0.3], "text-embedding-3-small")
    }
}

impl Default for EmbeddedChunk {
    fn default() -> Self {
        Self::test()
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

    pub fn test() -> Self {
        Self::new(EmbeddedChunk::test(), "test_id")
    }
}

impl Default for StoredChunk {
    fn default() -> Self {
        Self::test()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn can_create_document_with_builder() {
        let fixture = Document::default()
            .path("main.rs")
            .content("println!(\"test\");");

        let actual = fixture.path;
        let expected = PathBuf::from("main.rs");
        assert_eq!(actual, expected);
    }

    #[test]
    fn document_metadata_calculates_hash() {
        let content = "test content";
        let fixture = DocumentMetadata::new(&PathBuf::from("test.rs"), content);

        let actual = fixture.hash;
        let expected = blake3::hash(content.as_bytes()).to_hex().to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn chunk_preserves_content() {
        let content = "fn test() {}";
        let fixture = Chunk::new(content, ChunkSource::test(), Position::test());

        let actual = fixture.content;
        let expected = content.to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn embedded_chunk_includes_model_info() {
        let model = "test-model";
        let fixture = EmbeddedChunk::new(Chunk::test(), vec![1.0, 2.0], model);

        let actual = fixture.embedding_model;
        let expected = model.to_string();
        assert_eq!(actual, expected);
    }
}
