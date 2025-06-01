use anyhow::Result;
use tracing::info;

use super::{Cache, Embedder, EmbedderInput, EmbedderOutput};

/// A wrapper that adds caching capabilities to any embedder
#[derive(Clone)]
pub struct CachedEmbedder<E, C> {
    embedder: E,
    cache: C,
    model: String,
    dims: u32,
}

impl<E, C> CachedEmbedder<E, C> {
    /// Create a new cached embedder
    pub fn new(embedder: E, cache: C, model_name: impl Into<String>, dims: u32) -> Self {
        let model_name = model_name.into();
        Self { embedder, cache, model: model_name, dims }
    }

    /// Generate a cache key for a given model, dimensions, and content hash
    pub fn generate_key(&self, content_hash: &str) -> String {
        format!("{}.{}:{}", self.model, self.dims, content_hash)
    }
}

#[async_trait::async_trait]
impl<E, C> Embedder for CachedEmbedder<E, C>
where
    E: Embedder + Send + Sync,
    C: Cache<Key = String, Value = Vec<f32>> + Send + Sync,
{
    async fn embed<T>(&self, inputs: Vec<EmbedderInput<T>>) -> Result<Vec<EmbedderOutput>>
    where
        T: ToString + Send,
    {
        info!("Checking cached embeddings");
        let mut result = vec![None; inputs.len()];

        let inputs = inputs
            .into_iter()
            .map(|input: EmbedderInput<T>| EmbedderInput { payload: input.payload.to_string() })
            .collect::<Vec<_>>();

        let mut uncached = Vec::new();

        // Check cache first
        let mut cache_hits = 0;

        // Check which inputs are already cached
        for (i, block) in inputs.iter().enumerate() {
            let cache_key = self.generate_key(&block.hash());
            match self.cache.get(cache_key).await {
                Ok(Some(embeddings)) => {
                    cache_hits += 1;
                    result.insert(i, Some(EmbedderOutput { embeddings }));
                }
                _ => {
                    uncached.push((i, block));
                }
            }
        }

        info!("Uncached blocks: {}", uncached.len());

        // Process uncached items if any
        if !uncached.is_empty() {
            // Extract just the uncached inputs for the underlying embedder
            let uncached_inputs = uncached
                .iter()
                .map(|(_, block)| EmbedderInput { payload: block.payload.clone() })
                .collect::<Vec<_>>();

            // Get embeddings from the underlying embedder
            let uncached_embeddings = self.embedder.embed(uncached_inputs).await?;

            for (embeddings, (position, block)) in
                uncached_embeddings.into_iter().zip(uncached.into_iter())
            {
                let cache_key = self.generate_key(&block.hash());
                let _ = self
                    .cache
                    .put(cache_key, embeddings.embeddings.clone())
                    .await;
                result.insert(position, Some(embeddings));
            }
        }

        info!("Cache hits: {}/{}", cache_hits, inputs.len());

        Ok(result.into_iter().flatten().collect())
    }
}
