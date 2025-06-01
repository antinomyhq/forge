use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_treesitter::{Block, Offset, Span};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::chunkers::Chunker;
use crate::embedders::Embedder;
use crate::loaders::Loader;
use crate::{CachedEmbedder, DiskCache, EmbedderInput, Store, StoreInput};

impl From<&forge_treesitter::Block> for EmbedderInput<String> {
    fn from(block: &forge_treesitter::Block) -> Self {
        // We only want to generate embeddings for code and it's relative path
        let payload = serde_json::json!({
            "path": block.relative_path(),
            "code": block.snippet,
        })
        .to_string();

        Self { payload }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MetaData {
    path: String,
    kind: String,
    offset: Offset,
    span: Span,
}

impl From<Block> for MetaData {
    fn from(value: Block) -> Self {
        Self {
            path: value.path.display().to_string(),
            kind: value.kind.to_string(),
            offset: value.offset,
            span: value.span,
        }
    }
}

/// Indexer for indexing files
pub struct Orchestrator<L: Loader, C: Chunker, E: Embedder, S: Store> {
    pub loader: Arc<L>,
    pub chunker: Arc<C>,
    pub embedder: Arc<E>,
    pub store: Arc<S>,
}

use crate::{FileLoader, HnswStore, OpenAI, QueryOptions, QueryOutput, TreeSitterChunker};
impl Default
    for Orchestrator<
        FileLoader,
        TreeSitterChunker,
        CachedEmbedder<OpenAI, DiskCache<String, Vec<f32>>>,
        HnswStore<'_>,
    >
{
    fn default() -> Self {
        let embedding_model = "text-embedding-3-large";
        let embedding_dims = 3072;
        let max_tokens_supported = 8192;

        let cache_dir = format!("{}:{}", embedding_model.replace("/", "-"), embedding_dims);
        let cache_path = std::env::current_dir()
            .expect("failed to retrive current working directory.")
            .join(PathBuf::from(format!("./.cache/embeddings/{cache_dir}")));

        let loader = FileLoader::default();
        let chunker = TreeSitterChunker::try_new(embedding_model, max_tokens_supported)
            .expect("failed to create chunker");

        let embedder = OpenAI::new(embedding_model, embedding_dims);
        let cache: DiskCache<String, Vec<f32>> = DiskCache::try_new(cache_path).unwrap();
        let embedding_cache = CachedEmbedder::new(embedder, cache, embedding_model, embedding_dims);

        let store = HnswStore::new(embedding_dims as usize);

        Self {
            loader: Arc::new(loader),
            chunker: Arc::new(chunker),
            embedder: Arc::new(embedding_cache),
            store: Arc::new(store),
        }
    }
}

impl<L: Loader, C: Chunker, E: Embedder, S: Store> Orchestrator<L, C, E, S> {
    pub fn new(loader: Arc<L>, chunker: Arc<C>, embedder: Arc<E>, store: Arc<S>) -> Self {
        Self { loader, chunker, embedder, store }
    }
}

impl<L: Loader, C: Chunker, E: Embedder, S: Store> Orchestrator<L, C, E, S> {
    pub async fn index(&self, path: &Path) -> anyhow::Result<()> {
        // Firstly reset the store to avoid duplicate records.
        // TODO: this is naive approach to avoid duplicate records.
        self.store.reset().await?;

        let files = self.loader.load(path).await?;
        let code_blocks = self.chunker.chunk(files).await?;
        let embeddings = self
            .embedder
            .embed::<String>(code_blocks.iter().map(Into::into).collect())
            .await?;

        self.store
            .store(
                code_blocks
                    .into_iter()
                    .zip(embeddings.into_iter())
                    .map(|(block, embeddings)| {
                        Ok(StoreInput {
                            embeddings: embeddings.embeddings,
                            metadata: serde_json::to_value(MetaData::from(block))?,
                        })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?,
            )
            .await?;
        Ok(())
    }

    pub async fn query<P: Send + Sync + DeserializeOwned>(
        &self,
        query: &str,
        options: QueryOptions,
    ) -> anyhow::Result<Vec<QueryOutput<P>>> {
        let embeddings = self
            .embedder
            .embed::<String>(vec![EmbedderInput { payload: query.to_string() }])
            .await?;
        if let Some(query_embeddings) = embeddings.into_iter().next() {
            self.store.query(query_embeddings.embeddings, options).await
        } else {
            Err(anyhow::anyhow!("No embeddings returned from embedder"))
        }
    }
}
