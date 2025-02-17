use std::sync::Arc;

use anyhow::{anyhow, Context};
use forge_app::KnowledgeRepository;
use forge_domain::{Environment, Knowledge, Query};
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, PointStruct, SearchPointsBuilder, UpsertPointsBuilder,
    VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant, QdrantError};
use serde_json::Value;
use tokio::sync::Mutex;
use tonic::Code;

pub struct QdrantKnowledgeRepository {
    env: Environment,
    client: Arc<Mutex<Option<Arc<Qdrant>>>>,
    collection: String,
    size: u64,
}

impl QdrantKnowledgeRepository {
    pub fn new(env: Environment, collection: impl ToString, size: u64) -> Self {
        Self {
            env,
            client: Default::default(),
            collection: collection.to_string(),
            size,
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

            // Attempt to create collection directly - handle "already exists" case
            match client
                .create_collection(
                    CreateCollectionBuilder::new(self.collection.clone())
                        .vectors_config(VectorParamsBuilder::new(self.size, Distance::Cosine)),
                )
                .await
            {
                Ok(_) => (),
                Err(e) => {
                    match e {
                        QdrantError::ResponseError { status }
                            if status.code() == Code::AlreadyExists =>{}
                        _ => {
                            return Err(e).with_context(|| {
                                format!("Failed to create collection: {}", self.collection)
                            });
                        }
                    }
                }
            }

            *guard = Some(client.clone());

            Ok(client)
        }
    }
}

#[async_trait::async_trait]
impl KnowledgeRepository<Value> for QdrantKnowledgeRepository {
    async fn store(&self, info: Vec<Knowledge<Value>>) -> anyhow::Result<()> {
        let points = info
            .into_iter()
            .map(|info| {
                let id = info.id.into_uuid().to_string();
                let vectors = info.embedding;
                let payload: anyhow::Result<Payload> = Ok(serde_json::from_value(info.content)?);
                Ok(PointStruct::new(id, vectors, payload?))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        // dbg!("storage",&points.len());
        self.client()
            .await?
            .upsert_points(UpsertPointsBuilder::new(self.collection.clone(), points))
            .await
            .with_context(|| {
                format!("Failed to upsert points to collection: {}", self.collection)
            })?;

        Ok(())
    }

    async fn search(&self, query: Query) -> anyhow::Result<Vec<Value>> {
        let points = SearchPointsBuilder::new(
            self.collection.clone(),
            query.embedding,
            query.limit.unwrap_or(10),
        ).with_payload(true); 
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
            .map(|point| Ok(serde_json::to_value(point.payload)?))
            .collect::<anyhow::Result<Vec<_>>>()
    }
}
