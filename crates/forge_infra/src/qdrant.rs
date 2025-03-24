use std::sync::Arc;

use anyhow::{anyhow, Context};
use forge_domain::{Environment, Point, Query};
use forge_services::VectorIndex;
use qdrant_client::qdrant::{PointStruct, SearchPointsBuilder, UpsertPointsBuilder};
use qdrant_client::{Payload, Qdrant};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::Mutex;

pub struct QdrantVectorIndex {
    env: Environment,
    client: Arc<Mutex<Option<Arc<Qdrant>>>>,
    collection: String,
}

impl QdrantVectorIndex {
    pub fn new(env: Environment, collection: impl ToString) -> Self {
        Self {
            env,
            client: Default::default(),
            collection: collection.to_string(),
        }
    }

    async fn client(&self) -> anyhow::Result<Arc<Qdrant>> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            Ok(client.clone())
        } else {
            let client = Arc::new(
                Qdrant::from_url(
                    self.env
                        .qdrant_cluster
                        .as_ref()
                        .ok_or(anyhow!("Qdrant Cluster is not set"))?,
                )
                .api_key(
                    self.env
                        .qdrant_key
                        .as_ref()
                        .ok_or(anyhow!("Qdrant Key is not set"))?
                        .as_str(),
                )
                .build()
                .with_context(|| "Failed to connect to knowledge service")?,
            );

            *guard = Some(client.clone());

            Ok(client)
        }
    }
}

#[async_trait::async_trait]
impl<T: Serialize + DeserializeOwned + Send + Sync + 'static> VectorIndex<T> for QdrantVectorIndex {
    async fn store(&self, info: Point<T>) -> anyhow::Result<()> {
        let id = info.id.into_uuid().to_string();
        let vectors = info.embedding;

        let mut payload = Payload::new();
        payload.insert("content", serde_json::to_string(&info.content)?);

        let point = PointStruct::new(id, vectors, payload);
        self.client()
            .await?
            .upsert_points(UpsertPointsBuilder::new(
                self.collection.clone(),
                vec![point],
            ))
            .await
            .with_context(|| {
                format!("Failed to upsert points to collection: {}", self.collection)
            })?;

        Ok(())
    }

    async fn search(&self, query: Query) -> anyhow::Result<Vec<Point<T>>> {
        let points = SearchPointsBuilder::new(
            self.collection.clone(),
            query.embedding,
            query.limit.unwrap_or(10),
        )
        .with_payload(true);
        let results = self
            .client()
            .await?
            .search_points(points)
            .await
            .with_context(|| {
                format!("Failed to search points in collection: {}", self.collection)
            })?;

        results
            .result
            .into_iter()
            .map(|point| {
                let content = point.payload.get("content").unwrap().clone();
                Ok(serde_json::from_str(content.as_str().unwrap())?)
            })
            .collect::<anyhow::Result<Vec<_>>>()
    }
}
