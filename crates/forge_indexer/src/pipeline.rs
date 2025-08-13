use derive_setters::Setters;
use futures::{StreamExt, TryStreamExt};
use tokio_stream;

use crate::traits::{Chunker, Embedder, Loader, StorageWriter};

/// Configuration for the indexing pipeline
#[derive(Debug, Clone, Setters)]
#[setters(strip_option, into)]
pub struct PipelineConfig {
    /// Batch size for embedding operations
    pub embed_batch_size: usize,
    /// Maximum number of concurrent embedding operations
    pub max_concurrent_embeds: usize,
    /// Batch size for storage operations
    pub storage_batch_size: usize,
    /// Maximum number of concurrent storage operations
    pub max_concurrent_storage: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            embed_batch_size: 10,
            max_concurrent_embeds: 3,
            storage_batch_size: 50,
            max_concurrent_storage: 2,
        }
    }
}

/// A generic indexing pipeline that uses stream processing for scalable
/// document indexing
///
/// The pipeline follows this flow:
/// 1. Load documents from a source (Loader)
/// 2. Chunk documents into smaller pieces (Chunker)
/// 3. Generate embeddings for chunks in batches (Embedder)
/// 4. Store embedded chunks with metadata (StorageWriter)
///
/// The pipeline uses proper stream processing with backpressure control to
/// handle large datasets efficiently without overwhelming system resources.
#[derive(Debug)]
pub struct IndexingPipeline<L, C, E, S> {
    loader: L,
    chunker: C,
    embedder: E,
    storage: S,
    config: PipelineConfig,
}

impl<L, C, E, S> IndexingPipeline<L, C, E, S>
where
    L: Loader,
    C: Chunker<Input = L::Item>,
    E: Embedder<Input = Vec<C::Output>>,
    S: StorageWriter<Input = E::Output>,
{
    /// Create a new indexing pipeline with the given components
    pub fn new(loader: L, chunker: C, embedder: E, storage: S) -> Self {
        Self {
            loader,
            chunker,
            embedder,
            storage,
            config: PipelineConfig::default(),
        }
    }

    /// Create a new indexing pipeline with custom configuration
    pub fn with_config(
        loader: L,
        chunker: C,
        embedder: E,
        storage: S,
        config: PipelineConfig,
    ) -> Self {
        Self { loader, chunker, embedder, storage, config }
    }

    /// Process all documents and return a summary of the indexing process
    ///
    /// This method processes the entire stream and collects all results.
    /// For large datasets, consider using the `stream()` method instead.
    pub async fn index(&self) -> anyhow::Result<Vec<S::Output>> {
        self.stream().try_collect().await
    }

    /// Create a streaming version of the pipeline that yields results as
    /// they're processed
    ///
    /// This is useful for real-time processing or when you want to handle
    /// results incrementally. The stream applies proper backpressure
    /// control at each stage.
    pub fn stream(&self) -> impl futures::Stream<Item = anyhow::Result<S::Output>> + '_ {
        let chunker = &self.chunker;
        let embedder = &self.embedder;
        let storage = &self.storage;
        let config = &self.config;

        self.loader
            .load()
            // Flat-map each document into a stream of chunks
            .map_ok(move |document| {
                let chunks = chunker.chunk(document);
                tokio_stream::iter(chunks.into_iter().map(Ok::<C::Output, anyhow::Error>))
            })
            .try_flatten()
            // Batch chunks for embedding
            .try_chunks(config.embed_batch_size)
            .map(|chunk_batch_result| {
                match chunk_batch_result {
                    Ok(chunk_batch) => chunk_batch,
                    Err(_) => Vec::new(), // Return empty vec on error
                }
            })
            // Process embedding batches with controlled concurrency
            .map(|chunk_batch| {
                let embedder = embedder.clone();
                async move { embedder.embed(chunk_batch).await }
            })
            .buffer_unordered(config.max_concurrent_embeds)
            // Store individual embedded items with controlled concurrency
            .map(move |embedded_result| {
                let storage = storage;
                async move {
                    match embedded_result {
                        Ok(embedded_item) => storage.write(embedded_item).await,
                        Err(e) => Err(e),
                    }
                }
            })
            .buffer_unordered(config.max_concurrent_storage)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use futures::StreamExt;
    use tokio_stream;

    use super::*;

    // Test fixtures
    #[derive(Debug, Clone)]
    struct TestLoader {
        documents: Vec<String>,
    }

    impl TestLoader {
        fn new(documents: Vec<String>) -> Self {
            Self { documents }
        }
    }

    #[async_trait]
    impl Loader for TestLoader {
        type Item = String;
        type Stream = Box<dyn futures::Stream<Item = anyhow::Result<Self::Item>> + Send + Unpin>;

        fn load(&self) -> Self::Stream {
            let stream = tokio_stream::iter(
                self.documents
                    .clone()
                    .into_iter()
                    .map(Ok::<String, anyhow::Error>),
            );
            Box::new(stream)
        }
    }

    #[derive(Debug, Clone)]
    struct TestChunker {
        chunk_size: usize,
    }

    impl TestChunker {
        fn new(chunk_size: usize) -> Self {
            Self { chunk_size }
        }
    }

    impl Chunker for TestChunker {
        type Input = String;
        type Output = String;

        fn chunk(&self, input: Self::Input) -> Vec<Self::Output> {
            input
                .chars()
                .collect::<Vec<_>>()
                .chunks(self.chunk_size)
                .map(|chunk| chunk.iter().collect())
                .collect()
        }
    }

    #[derive(Debug, Clone)]
    struct TestEmbedder;

    #[async_trait]
    impl Embedder for TestEmbedder {
        type Input = Vec<String>;
        type Output = (String, Vec<f32>);

        async fn embed(&self, input: Self::Input) -> anyhow::Result<Self::Output> {
            let combined = input.join("");
            let embedding = vec![combined.len() as f32; 3];
            Ok((combined, embedding))
        }
    }

    #[derive(Debug, Clone)]
    struct TestStorageWriter;

    #[async_trait]
    impl StorageWriter for TestStorageWriter {
        type Input = (String, Vec<f32>);
        type Output = String;

        async fn write(&self, input: Self::Input) -> anyhow::Result<Self::Output> {
            let (text, embedding) = input;
            Ok(format!("stored: {} (embedding: {:?})", text, embedding))
        }
    }

    #[tokio::test]
    async fn test_pipeline_processes_documents() {
        let loader = TestLoader::new(vec!["hello".to_string(), "world".to_string()]);
        let chunker = TestChunker::new(3);
        let embedder = TestEmbedder;
        let storage = TestStorageWriter;

        let config = PipelineConfig::default()
            .embed_batch_size(2_usize)
            .storage_batch_size(2_usize);

        let pipeline = IndexingPipeline::with_config(loader, chunker, embedder, storage, config);

        let results = pipeline.index().await.unwrap();

        // Should have processed chunks from both documents
        assert!(!results.is_empty());

        // All results should start with "stored:"
        for result in &results {
            assert!(result.starts_with("stored:"));
        }
    }

    #[tokio::test]
    async fn test_pipeline_streaming() {
        let loader = TestLoader::new(vec!["test".to_string()]);
        let chunker = TestChunker::new(2);
        let embedder = TestEmbedder;
        let storage = TestStorageWriter;

        let pipeline = IndexingPipeline::new(loader, chunker, embedder, storage);

        let mut results = Vec::new();
        let mut stream = pipeline.stream();

        while let Some(result) = stream.next().await {
            results.push(result.unwrap());
        }

        assert!(!results.is_empty());
        for result in &results {
            assert!(result.starts_with("stored:"));
        }
    }
}
