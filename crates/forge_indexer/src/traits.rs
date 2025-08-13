use async_trait::async_trait;
use futures::Stream;

/// Trait for loading documents from various sources
#[async_trait]
pub trait Loader {
    type Item: Send + 'static;
    type Stream: Stream<Item = anyhow::Result<Self::Item>> + Send + Unpin;

    fn load(&self) -> Self::Stream;
}

/// Trait for chunking documents into smaller pieces
pub trait Chunker: Send + Sync {
    type Input: Send;
    type Output: Send + Clone;

    fn chunk(&self, input: Self::Input) -> Vec<Self::Output>;
}

/// Trait for creating embeddings from chunks
#[async_trait]
pub trait Embedder: Send + Sync + Clone {
    type Input: Send;
    type Output: Send + Clone;
    async fn embed(&self, inputs: Self::Input) -> anyhow::Result<Self::Output>;
}

/// Trait for storing embedded chunks
#[async_trait]
pub trait StorageWriter: Send + Sync {
    type Input: Send;
    type Output: Send;

    async fn write(&self, input: Self::Input) -> anyhow::Result<Self::Output>;
}

/// Trait for querying embedded chunks
#[async_trait]
pub trait StorageReader: Send + Sync {
    type Query: Send;
    type QueryResult: Send;
    // Query interface
    async fn query(&self, query: Self::Query) -> anyhow::Result<Self::QueryResult>;
}
