mod chunkers;
mod embedders;
mod loaders;
mod orchestrator;
mod stores;
mod token_counter;

pub use chunkers::*;
pub use embedders::*;
pub use loaders::*;
pub use orchestrator::*;
pub use stores::*;
pub use token_counter::*;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::{
        CachedEmbedder, DiskCache, FileLoader, HnswStore, OpenAI, Orchestrator, QueryOptions,
        TreeSitterChunker,
    };

    #[tokio::test]
    async fn test_indexer() {
        dotenv::dotenv().ok();

        let embedding_model = "text-embedding-3-small";
        let embedding_dimensions = 1536;

        let cache_dir = format!(
            "{}:{}",
            embedding_model.replace("/", "-"),
            embedding_dimensions
        );
        let cwd = std::env::current_dir().expect("failed to retrive current working directory.");
        let cache_path = cwd.join(PathBuf::from(format!("./.cache/embeddings/{}", cache_dir)));

        let loader = FileLoader::default();
        let chunker = TreeSitterChunker::try_new(embedding_model, 8192).unwrap();
        let embedder = OpenAI::new(embedding_model, embedding_dimensions);
        let cache: DiskCache<String, Vec<f32>> = DiskCache::try_new(cache_path).unwrap();
        let embedding_cache =
            CachedEmbedder::new(embedder, cache, embedding_model, embedding_dimensions);

        let hnsw_store = HnswStore::new(embedding_dimensions as usize);

        let indexer: Orchestrator<
            FileLoader,
            TreeSitterChunker,
            CachedEmbedder<OpenAI, DiskCache<String, Vec<f32>>>,
            HnswStore<'_>,
        > = Orchestrator::new(
            Arc::new(loader),
            Arc::new(chunker),
            Arc::new(embedding_cache),
            Arc::new(hnsw_store),
        );

        let _ = indexer.index(&cwd.join("src/lib.rs")).await.unwrap();

        let query = "In indexer test, what's embedding provider being used?";
        let result = indexer
            .query::<serde_json::Value>(query, QueryOptions::default())
            .await
            .unwrap();
        assert!(!result.is_empty());
        assert_eq!(result.len(), 1)
    }
}
