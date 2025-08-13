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
        
        let result = self
            .loader
            .load()
            // Stage 1: Flatten documents into individual chunks
            .map(|document_result| {
                let chunker = &self.chunker;
                async move {
                    match document_result {
                        Ok(document) => {
                            let chunks = chunker.chunk(document);
                            Ok(stream::iter(chunks.into_iter().map(Ok::<_, anyhow::Error>)))
                        }
                        Err(e) => Err(e),
                    }
                }
            })
            .buffer_unordered(self.config.max_concurrent_chunks)
            .try_flatten()
            // Stage 2: Batch chunks for embedding using ready_chunks
            .ready_chunks(self.config.embed_batch_size)
            .map(|chunk_batch| {
                let embedder = self.embedder.clone();
                async move {
                    // Extract successful chunks from results
                    let chunks: Result<Vec<_>, _> = chunk_batch.into_iter().collect();
                    let chunks = chunks?;
                    embedder.embed_batch(chunks).await
                }
            })
            .buffer_unordered(self.config.max_concurrent_embeds)
            // Stage 3: Store embedded batches immediately
            .map(|embedded_batch_result| {
                let storage_writer = &self.storage_writer;
                async move {
                    let embedded_batch = embedded_batch_result?;
                    // Process in storage batches for efficiency
                    let mut results = Vec::new();
                    for storage_batch in embedded_batch.chunks(self.config.storage_batch_size) {
                        let batch_results = storage_writer.store_batch(storage_batch.to_vec()).await?;
                        results.extend(batch_results);
                    }
                    Ok::<Vec<SW::Output>, anyhow::Error>(results)
                }
            })
            .buffer_unordered(self.config.max_concurrent_storage)
            .try_collect::<Vec<Vec<SW::Output>>>()
            .await?
            .into_iter()
            .flatten()
            .collect();

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
