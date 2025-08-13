# Forge Indexer Design Proposal

## Overview
This design proposes a flexible, trait-based architecture for the indexing pipeline that allows for pluggable components while maintaining high performance through streaming and batching.

## Core Architecture

### 1. Pipeline Abstraction
```rust
use futures::{Stream, StreamExt, TryStreamExt};
use tokio_stream;

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
    C: Chunker,
    E: Embedder,
    SW: StorageWriter,
{
    pub async fn index(&self) -> anyhow::Result<Vec<SW::Output>> {
        let result = self.loader
            .load()
            .flat_map(|document| {
                let chunks = self.chunker.chunk(document);
                tokio_stream::iter(chunks)
            })
            .ready_chunks(self.config.embed_batch_size)
            .map(|chunk_batch| self.embedder.embed_batch(chunk_batch))
            .buffer_unordered(self.config.max_concurrent_embeds)
            .map(|embedded_batch_result| {
                let storage_writer = &self.storage_writer;
                async move {
                    let embedded_batch = embedded_batch_result?;
                    storage_writer.store_batch(embedded_batch).await
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
```

### 2. Core Traits

#### Loader Trait
```rust
use futures::Stream;

#[async_trait]
pub trait Loader {
    type Item: Send + 'static;
    type Stream: Stream<Item = anyhow::Result<Self::Item>> + Send + Unpin;
    
    fn load(&self) -> Self::Stream;
}
```

#### Chunker Trait
```rust
pub trait Chunker: Send + Sync {
    type Input: Send;
    type Output: Send;
    
    fn chunk(&self, input: Self::Input) -> Vec<Self::Output>;
}
```

#### Embedder Trait
```rust
#[async_trait]
pub trait Embedder: Send + Sync + Clone {
    type Input: Send;
    type Output: Send;
    
    async fn embed_batch(&self, inputs: Vec<Self::Input>) -> anyhow::Result<Vec<Self::Output>>;
    
    fn batch_size(&self) -> usize;
}
```

#### Storage Writer Trait
```rust
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
```

#### Storage Reader Trait
```rust
#[async_trait]
pub trait StorageReader: Send + Sync {
    type Query: Send;
    type QueryResult: Send;
    
    // Query interface
    async fn query(&self, query: Self::Query) -> anyhow::Result<Vec<Self::QueryResult>>;
}
```

### 3. Domain Types

#### Core Document Types
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub path: PathBuf,
    pub content: String,
    pub metadata: DocumentMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub file_type: String,
    pub size: usize,
    pub modified: SystemTime,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub content: String,
    pub source: ChunkSource,
    pub position: Position,
    pub metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSource {
    pub document_path: PathBuf,
    pub document_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub start_char: usize,
    pub end_char: usize,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub chunk_type: ChunkType,
    pub language: Option<String>,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkType {
    Code,
    Documentation,
    Configuration,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedChunk {
    pub chunk: Chunk,
    pub embedding: Vec<f32>,
    pub embedding_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChunk {
    pub embedded_chunk: EmbeddedChunk,
    pub storage_id: String,
    pub stored_at: SystemTime,
}
```

### 4. Configuration
```rust
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub chunk_batch_size: usize,
    pub embed_batch_size: usize,
    pub storage_batch_size: usize,
    pub max_concurrent_chunks: usize,
    pub max_concurrent_embeds: usize,
    pub max_concurrent_storage: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            chunk_batch_size: 50,
            embed_batch_size: 100,
            storage_batch_size: 100,
            max_concurrent_chunks: 10,
            max_concurrent_embeds: 5,
            max_concurrent_storage: 5,
        }
    }
}
```

### 5. Concrete Implementations

#### File System Loader
```rust
use tokio_stream::{StreamExt, wrappers::ReadDirStream};

pub struct FileSystemLoader {
    pub root_path: PathBuf,
    pub extensions: Vec<String>,
    pub ignore_patterns: Vec<String>,
}

impl Loader for FileSystemLoader {
    type Item = Document;
    type Stream = impl Stream<Item = anyhow::Result<Document>> + Send + Unpin;
    
    fn load(&self) -> Self::Stream {
        let root_path = self.root_path.clone();
        let extensions = self.extensions.clone();
        let ignore_patterns = self.ignore_patterns.clone();
        
        async_stream::stream! {
            let walker = walkdir::WalkDir::new(root_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|entry| entry.file_type().is_file())
                .filter(move |entry| {
                    let path = entry.path();
                    
                    // Check extensions
                    if !extensions.is_empty() {
                        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                            if !extensions.contains(&ext.to_string()) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    
                    // Check ignore patterns
                    let path_str = path.to_string_lossy();
                    !ignore_patterns.iter().any(|pattern| path_str.contains(pattern))
                });
            
            for entry in walker {
                let path = entry.path().to_path_buf();
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => {
                        let metadata = DocumentMetadata {
                            file_type: path.extension()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            size: content.len(),
                            modified: entry.metadata()
                                .and_then(|m| m.modified())
                                .unwrap_or(SystemTime::UNIX_EPOCH),
                            hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
                        };
                        
                        yield Ok(Document {
                            path,
                            content,
                            metadata,
                        });
                    }
                    Err(e) => yield Err(anyhow::anyhow!("Failed to read file {}: {}", path.display(), e)),
                }
            }
        }
    }
}
```

#### Code Chunking Strategy
The chunking component will utilize the `text-splitter` crate with tree-sitter integration to create semantic code chunks. This approach provides:

- **Semantic Awareness**: Chunks respect code structure (functions, classes, modules)
- **Language Support**: Tree-sitter grammars for multiple programming languages  
- **Configurable Sizing**: Line-based or token-based chunk sizing with overlap
- **Preservation of Context**: Maintains syntactic completeness within chunks

##### Key Design Decisions:
1. **Tree-sitter Integration**: Leverages abstract syntax trees for intelligent chunk boundaries
2. **Language Detection**: Automatic language detection based on file extensions
3. **Configurable Overlap**: Maintains context across chunk boundaries
4. **Chunk Metadata**: Rich metadata including language, position, and content hash

##### Supported Languages:
- Rust (primary focus)
- JavaScript/TypeScript
- Python
- Go
- Java
- C/C++
- Additional languages can be added via tree-sitter grammars

The chunker trait abstracts the chunking strategy while concrete implementations leverage tree-sitter's parsing capabilities to understand code structure and create meaningful chunk boundaries that preserve semantic meaning.
}
```

#### OpenAI Embedder
```rust
pub struct OpenAIEmbedder {
    client: Client<OpenAIConfig>,
    model: String,
    batch_size: usize,
}

impl Embedder for OpenAIEmbedder {
    type Input = Chunk;
    type Output = EmbeddedChunk;
    
    async fn embed_batch(&self, chunks: Vec<Chunk>) -> anyhow::Result<Vec<EmbeddedChunk>> {
        // Implementation using OpenAI API
    }
}
```

#### Qdrant Storage Writer
```rust
pub struct QdrantStorageWriter {
    client: Qdrant,
    collection_name: String,
    batch_size: usize,
}

impl StorageWriter for QdrantStorageWriter {
    type Input = EmbeddedChunk;
    type Output = StoredChunk;
    
    async fn store(&self, chunk: EmbeddedChunk) -> anyhow::Result<StoredChunk> {
        // Implementation using Qdrant
    }
    
    async fn store_batch(&self, chunks: Vec<EmbeddedChunk>) -> anyhow::Result<Vec<StoredChunk>> {
        // Implementation using Qdrant
    }
    
    fn batch_size(&self) -> usize {
        self.batch_size
    }
}
```

#### Qdrant Storage Reader
```rust
pub struct QdrantStorageReader {
    client: Qdrant,
    collection_name: String,
}

impl StorageReader for QdrantStorageReader {
    type Query = SearchQuery;
    type QueryResult = SearchResult;
    
    async fn query(&self, query: SearchQuery) -> anyhow::Result<Vec<SearchResult>> {
        // Implementation using Qdrant
    }
}
```

### 6. Builder Pattern for Easy Construction
```rust
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

// Similar implementations for other components...

impl<L, C, E, SW> PipelineBuilder<L, C, E, SW>
where
    L: Loader,
    C: Chunker<Input = L::Item>,
    E: Embedder<Input = C::Output>,
    SW: StorageWriter<Input = E::Output>,
{
    pub fn build(self) -> IndexingPipeline<L, C, E, SW> {
        IndexingPipeline {
            loader: self.loader,
            chunker: self.chunker,
            embedder: self.embedder,
            storage_writer: self.storage_writer,
            config: self.config,
        }
    }
}
```

### Query Pipeline for Search Operations
```rust
pub struct QueryPipeline<E, SR> {
    embedder: E,
    storage_reader: SR,
}

impl<E, SR> QueryPipeline<E, SR>
where
    E: Embedder,
    SR: StorageReader,
{
    pub fn new() -> QueryPipelineBuilder {
        QueryPipelineBuilder::new()
    }
    
    pub async fn search(&self, query: &str) -> anyhow::Result<Vec<SR::QueryResult>> {
        // Convert query to embedding and search
    }
}

pub struct QueryPipelineBuilder<E = (), SR = ()> {
    embedder: E,
    storage_reader: SR,
}

impl QueryPipelineBuilder {
    pub fn new() -> Self {
        Self {
            embedder: (),
            storage_reader: (),
        }
    }
}

impl<SR> QueryPipelineBuilder<(), SR> {
    pub fn with_embedder<E: Embedder>(self, embedder: E) -> QueryPipelineBuilder<E, SR> {
        QueryPipelineBuilder {
            embedder,
            storage_reader: self.storage_reader,
        }
    }
}

impl<E> QueryPipelineBuilder<E, ()> {
    pub fn with_storage_reader<SR: StorageReader>(self, storage_reader: SR) -> QueryPipelineBuilder<E, SR> {
        QueryPipelineBuilder {
            embedder: self.embedder,
            storage_reader,
        }
    }
}

impl<E, SR> QueryPipelineBuilder<E, SR>
where
    E: Embedder,
    SR: StorageReader,
{
    pub fn build(self) -> QueryPipeline<E, SR> {
        QueryPipeline {
            embedder: self.embedder,
            storage_reader: self.storage_reader,
        }
    }
}
```

### 7. Usage Example
```rust
// Create a pipeline with specific implementations
let pipeline = PipelineBuilder::new()
    .with_loader(FileSystemLoader {
        root_path: PathBuf::from("./src"),
        extensions: vec!["rs".to_string()],
        ignore_patterns: vec!["target".to_string()],
    })
    .with_chunker(TextSplitterChunker {
        strategy: ChunkingStrategy::ByLines { 
            max_lines: 50, 
            overlap_lines: 5 
        },
        config: ChunkConfig::default(),
    })
    .with_embedder(OpenAIEmbedder::new("text-embedding-3-small"))
    .with_storage_writer(QdrantStorageWriter::new("forge_code_chunks"))
    .with_config(PipelineConfig {
        embed_batch_size: 100,
        max_concurrent_embeds: 5,
        ..Default::default()
    })
    .build();

// Run the indexing
let result = pipeline.index().await?;

// Query the index
let query_pipeline = QueryPipeline::new()
    .with_embedder(OpenAIEmbedder::new("text-embedding-3-small"))
    .with_storage_reader(QdrantStorageReader::new("forge_code_chunks"));

let results = query_pipeline.search("async function implementation").await?;
```

## Benefits of This Design

1. **Modularity**: Each component can be swapped out independently
2. **Separation of Concerns**: Read and write operations are separated, allowing for different storage backends for indexing vs querying
3. **Testability**: Easy to mock individual components for testing
4. **Performance**: Maintains streaming and batching for optimal throughput
5. **Extensibility**: Easy to add new chunking strategies, embedders, or storage backends
6. **Type Safety**: Compile-time guarantees about component compatibility
7. **Configuration**: Flexible configuration for different use cases
8. **Scalability**: Write-heavy indexing and read-heavy querying can be optimized independently

## Implementation Strategy

1. Start with the core traits and domain types
2. Implement the pipeline framework with streaming
3. Create concrete implementations for current use case (FileSystem + TextSplitter + OpenAI + Qdrant)
4. Add builder pattern for easy construction
5. Add comprehensive tests with mock implementations
6. Add query pipeline and search functionality

This design maintains the performance benefits of your current implementation while providing the flexibility to swap out components as needed.