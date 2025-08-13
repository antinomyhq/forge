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

    async fn embed_batch(&self, inputs: Vec<Self::Input>) -> anyhow::Result<Vec<Self::Output>>;

    fn batch_size(&self) -> usize;
}

/// Trait for storing embedded chunks
#[async_trait]
pub trait StorageWriter: Send + Sync {
    type Input: Send;
    type Output: Send;

    async fn store(&self, input: Self::Input) -> anyhow::Result<Self::Output>;

    // Batch storage for efficiency
    async fn store_batch(&self, inputs: Vec<Self::Input>) -> anyhow::Result<Vec<Self::Output>>;

    // Get optimal batch size for this storage
    fn batch_size(&self) -> usize;
}

/// Trait for reading from storage (separate from writing for flexibility)
#[async_trait]
pub trait StorageReader: Send + Sync {
    type Query: Send;
    type QueryResult: Send;

    // Query interface
    async fn query(&self, query: Self::Query) -> anyhow::Result<Vec<Self::QueryResult>>;
}

#[cfg(test)]
mod tests {
    use crate::Chunker;

    #[test]
    fn traits_are_object_safe() {
        // This test ensures our traits can be used as trait objects
        let _: Option<Box<dyn Chunker<Input = String, Output = String>>> = None;
    }
}
