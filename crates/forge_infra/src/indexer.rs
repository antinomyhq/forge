use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_indexer::{
    CachedEmbedder, DiskCache, FileLoader, HnswStore, OpenAI, Orchestrator, QueryOptions,
    TreeSitterChunker,
};
use forge_services::IndexerService;
use reqwest::Url;
use serde::de::DeserializeOwned;

type EmeddingProvider = CachedEmbedder<OpenAI, DiskCache<String, Vec<f32>>>;
type CodeIndexingEngine =
    Orchestrator<FileLoader, TreeSitterChunker, EmeddingProvider, HnswStore<'static>>;

#[derive(Clone)]
pub struct ForgeCodeIndex(Arc<CodeIndexingEngine>);

impl ForgeCodeIndex {
    pub fn new(cwd: &Path, provide_url: Url, provider_key: String) -> Self {
        let embedding_model = "text-embedding-3-large";
        let embedding_dims = 3072;
        let max_tokens_supported = 8192;

        // disk cache path
        let cache_dir = format!("{}:{}", embedding_model.replace("/", "-"), embedding_dims);
        let cache_path = cwd.join(PathBuf::from(format!("./.cache/embeddings/{cache_dir}")));

        let loader = FileLoader::default();
        let chunker = TreeSitterChunker::try_new(embedding_model, max_tokens_supported)
            .expect("failed to create chunker");

        // cache for embeddings
        let embedder = OpenAI::new(embedding_model, embedding_dims)
            .with_api_key(provider_key)
            .with_base_url(provide_url);
        let cache: DiskCache<String, Vec<f32>> = DiskCache::try_new(cache_path).unwrap();
        let embedding_cache = CachedEmbedder::new(embedder, cache, embedding_model, embedding_dims);

        let store = HnswStore::new(embedding_dims as usize);

        Self(Arc::new(Orchestrator {
            loader: Arc::new(loader),
            chunker: Arc::new(chunker),
            embedder: Arc::new(embedding_cache),
            store: Arc::new(store),
        }))
    }
}

#[async_trait::async_trait]
impl IndexerService for ForgeCodeIndex {
    async fn index(&self, path: &Path) -> anyhow::Result<()> {
        self.0.index(path).await
    }

    async fn query<V: DeserializeOwned + Send + Sync>(
        &self,
        query: &str,
        options: forge_services::QueryOptions,
    ) -> anyhow::Result<Vec<V>> {
        let mut query_options = QueryOptions { limit: options.limit, ..Default::default() };

        if let Some(kind) = options.kind {
            query_options.kind = Some(kind);
        }
        if let Some(path) = options.path {
            query_options.path = Some(path);
        }
        let results = self.0.query::<V>(query, query_options).await?;
        Ok(results.into_iter().map(|output| output.payload).collect())
    }
}
