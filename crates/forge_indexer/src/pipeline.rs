use crate::config::PipelineConfig;
use crate::traits::*;

/// The main indexing pipeline that orchestrates the entire flow
pub struct IndexingPipeline<L, C, E, SW> {
    loader: L,
    chunker: C,
    embedder: E,
    storage_writer: SW,
    config: PipelineConfig,
}

impl<L, C, E, SW> IndexingPipeline<L, C, E, SW>
where
    L: Loader,
    C: Chunker<Input = L::Item>,
    E: Embedder<Input = C::Output>,
    SW: StorageWriter<Input = E::Output>,
{
    pub fn new(
        loader: L,
        chunker: C,
        embedder: E,
        storage_writer: SW,
        config: PipelineConfig,
    ) -> Self {
        Self { loader, chunker, embedder, storage_writer, config }
    }

    pub async fn index(&self) -> anyhow::Result<Vec<SW::Output>> {
        use futures::stream::{self, StreamExt, TryStreamExt};

        // Maximum parallelism pipeline - data flows through stages continuously like
        // sample.rs
        let result = self
            .loader
            .load()
            // Stage 1: Process documents into chunks immediately (no await, like sample)
            .flat_map(|document_result| {
                let chunker = &self.chunker;
                match document_result {
                    Ok(document) => {
                        let chunks = chunker.chunk(document);
                        let chunk_results: Vec<Result<_, anyhow::Error>> =
                            chunks.into_iter().map(Ok).collect();
                        stream::iter(chunk_results)
                    }
                    Err(e) => stream::iter(vec![Err(e)]),
                }
            })
            // Stage 2: Batch chunks for embedding
            .ready_chunks(self.config.embed_batch_size)
            .map(|chunk_batch| {
                let embedder = self.embedder.clone();
                // Call embed_batch directly, return the future - like sample.rs
                async move {
                    let chunks: Result<Vec<_>, _> = chunk_batch.into_iter().collect();
                    match chunks {
                        Ok(chunks) => embedder.embed_batch(chunks).await,
                        Err(e) => Err(e),
                    }
                }
            })
            .buffer_unordered(self.config.max_concurrent_embeds)
            // Stage 3: Flatten embedded batches immediately
            .flat_map(|embedded_batch_result| match embedded_batch_result {
                Ok(embedded_batch) => {
                    let embedded_results: Vec<Result<_, anyhow::Error>> =
                        embedded_batch.into_iter().map(Ok).collect();
                    stream::iter(embedded_results)
                }
                Err(e) => stream::iter(vec![Err(e)]),
            })
            // Stage 4: Batch for storage
            .ready_chunks(self.config.storage_batch_size)
            .map(|storage_batch| {
                let storage_writer = &self.storage_writer;
                async move {
                    let embedded_chunks: Result<Vec<_>, _> = storage_batch.into_iter().collect();
                    let embedded_chunks = embedded_chunks?;
                    storage_writer.store_batch(embedded_chunks).await
                }
            })
            .buffer_unordered(self.config.max_concurrent_storage)
            // Stage 5: Flatten storage results immediately
            .flat_map(|storage_result| match storage_result {
                Ok(stored_chunks) => {
                    let storage_results: Vec<Result<_, anyhow::Error>> =
                        stored_chunks.into_iter().map(Ok).collect();
                    stream::iter(storage_results)
                }
                Err(e) => stream::iter(vec![Err(e)]),
            })
            .try_collect::<Vec<SW::Output>>()
            .await?;

        Ok(result)
    }
}

/// Builder pattern for constructing indexing pipelines
pub struct PipelineBuilder<L = (), C = (), E = (), SW = ()> {
    loader: L,
    chunker: C,
    embedder: E,
    storage_writer: SW,
    config: PipelineConfig,
}

impl PipelineBuilder {
    pub fn new() -> PipelineBuilder {
        PipelineBuilder {
            loader: (),
            chunker: (),
            embedder: (),
            storage_writer: (),
            config: PipelineConfig::default(),
        }
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl<C, E, SW> PipelineBuilder<(), C, E, SW> {
    pub fn with_loader<L: Loader>(self, loader: L) -> PipelineBuilder<L, C, E, SW> {
        PipelineBuilder {
            loader,
            chunker: self.chunker,
            embedder: self.embedder,
            storage_writer: self.storage_writer,
            config: self.config,
        }
    }
}

impl<L, E, SW> PipelineBuilder<L, (), E, SW> {
    pub fn with_chunker<C: Chunker>(self, chunker: C) -> PipelineBuilder<L, C, E, SW> {
        PipelineBuilder {
            loader: self.loader,
            chunker,
            embedder: self.embedder,
            storage_writer: self.storage_writer,
            config: self.config,
        }
    }
}

impl<L, C, SW> PipelineBuilder<L, C, (), SW> {
    pub fn with_embedder<E: Embedder>(self, embedder: E) -> PipelineBuilder<L, C, E, SW> {
        PipelineBuilder {
            loader: self.loader,
            chunker: self.chunker,
            embedder,
            storage_writer: self.storage_writer,
            config: self.config,
        }
    }
}

impl<L, C, E> PipelineBuilder<L, C, E, ()> {
    pub fn with_storage_writer<SW: StorageWriter>(
        self,
        storage_writer: SW,
    ) -> PipelineBuilder<L, C, E, SW> {
        PipelineBuilder {
            loader: self.loader,
            chunker: self.chunker,
            embedder: self.embedder,
            storage_writer,
            config: self.config,
        }
    }
}

impl<L, C, E, SW> PipelineBuilder<L, C, E, SW> {
    pub fn with_config(mut self, config: PipelineConfig) -> Self {
        self.config = config;
        self
    }
}

impl<L, C, E, SW> PipelineBuilder<L, C, E, SW>
where
    L: Loader,
    C: Chunker<Input = L::Item>,
    E: Embedder<Input = C::Output>,
    SW: StorageWriter<Input = E::Output>,
{
    pub fn build(self) -> IndexingPipeline<L, C, E, SW> {
        IndexingPipeline::new(
            self.loader,
            self.chunker,
            self.embedder,
            self.storage_writer,
            self.config,
        )
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use futures::stream;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::domain::*;

    // Mock implementations for testing
    struct MockLoader;

    #[async_trait]
    impl Loader for MockLoader {
        type Item = Document;
        type Stream =
            std::pin::Pin<Box<dyn futures::Stream<Item = anyhow::Result<Document>> + Send + Unpin>>;

        fn load(&self) -> Self::Stream {
            Box::pin(stream::iter(vec![Ok(Document::test())]))
        }
    }

    struct MockChunker;

    impl Chunker for MockChunker {
        type Input = Document;
        type Output = Chunk;

        fn chunk(&self, _input: Document) -> Vec<Chunk> {
            vec![Chunk::test()]
        }
    }

    #[derive(Clone)]
    struct MockEmbedder;

    #[async_trait]
    impl Embedder for MockEmbedder {
        type Input = Chunk;
        type Output = EmbeddedChunk;

        async fn embed_batch(&self, inputs: Vec<Chunk>) -> anyhow::Result<Vec<EmbeddedChunk>> {
            Ok(inputs
                .into_iter()
                .map(|chunk| EmbeddedChunk::new(chunk, vec![0.1, 0.2, 0.3], "mock-model"))
                .collect())
        }

        fn batch_size(&self) -> usize {
            10
        }
    }

    struct MockStorageWriter;

    #[async_trait]
    impl StorageWriter for MockStorageWriter {
        type Input = EmbeddedChunk;
        type Output = StoredChunk;

        async fn store(&self, input: EmbeddedChunk) -> anyhow::Result<StoredChunk> {
            Ok(StoredChunk::new(input, "mock-id"))
        }

        async fn store_batch(
            &self,
            inputs: Vec<EmbeddedChunk>,
        ) -> anyhow::Result<Vec<StoredChunk>> {
            let mut results = Vec::new();
            for input in inputs {
                results.push(self.store(input).await?);
            }
            Ok(results)
        }

        fn batch_size(&self) -> usize {
            10
        }
    }

    #[tokio::test]
    async fn can_build_pipeline() {
        let fixture = PipelineBuilder::new()
            .with_loader(MockLoader)
            .with_chunker(MockChunker)
            .with_embedder(MockEmbedder)
            .with_storage_writer(MockStorageWriter)
            .with_config(PipelineConfig::test())
            .build();

        let actual = fixture.index().await.unwrap();
        let expected = 1; // Should have one stored chunk
        assert_eq!(actual.len(), expected);
    }

    #[test]
    fn builder_compiles_with_type_safety() {
        // This test ensures the builder pattern maintains type safety
        let _pipeline = PipelineBuilder::new()
            .with_loader(MockLoader)
            .with_chunker(MockChunker)
            .with_embedder(MockEmbedder)
            .with_storage_writer(MockStorageWriter);
    }
}
