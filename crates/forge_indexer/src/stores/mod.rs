mod hnsw;
mod qdrant;

use derive_setters::Setters;
pub use hnsw::*;
pub use qdrant::*;

pub struct StoreInput<T> {
    pub embeddings: Vec<f32>,
    pub metadata: T,
}

#[derive(Debug)]
pub struct QueryOutput<T> {
    pub score: f32,
    pub payload: T,
}

#[derive(Setters, Debug)]
#[setters(strip_option, into)]
pub struct QueryOptions {
    pub limit: u64,
    pub kind: Option<String>,
    pub path: Option<Vec<String>>,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self { limit: 10, kind: None, path: None }
    }
}

/// Store trait for storing embeddings
#[async_trait::async_trait]
pub trait Store: Send + Sync {
    async fn store<T>(&self, inputs: Vec<StoreInput<T>>) -> anyhow::Result<()>
    where
        T: Into<serde_json::Value> + Send + Sync;
    async fn query<T>(
        &self,
        query: Vec<f32>,
        options: QueryOptions,
    ) -> anyhow::Result<Vec<QueryOutput<T>>>
    where
        T: serde::de::DeserializeOwned + Send + Sync;
    async fn reset(&self) -> anyhow::Result<()>;
}
