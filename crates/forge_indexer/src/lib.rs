/// Core traits for the indexing pipeline
pub mod traits;

/// Domain types for documents, chunks, and embeddings
pub mod domain;

/// Configuration types
pub mod config;

pub mod chunker;
pub mod embedder;
/// File loader implementations
pub mod loader;
/// Generic pipeline for orchestrating the indexing workflow
pub mod pipeline;
pub mod qdrant;

pub use domain::*;
pub use loader::*;
pub use pipeline::*;
pub use traits::*;
