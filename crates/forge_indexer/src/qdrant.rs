use std::collections::HashMap;

use qdrant_client::Qdrant;
use qdrant_client::config::QdrantConfig;
use qdrant_client::qdrant::{PointStruct, UpsertPoints, Vector, Vectors};
use uuid::Uuid;

use crate::{EmbeddedChunk, StorageWriter};

pub struct QdrantStore {
    client: Qdrant,
    collection_name: String,
}

impl QdrantStore {
    pub fn try_new(api_key: String, url: String, collection_name: String) -> anyhow::Result<Self> {
        let client = QdrantConfig::from_url(&url).api_key(api_key).build()?;
        Ok(Self { client, collection_name })
    }
}

impl From<EmbeddedChunk> for PointStruct {
    fn from(chunk: EmbeddedChunk) -> Self {
        // Create a unique ID for each point using UUID
        let point_id = Uuid::new_v4().to_string();

        // Create payload with chunk metadata
        let mut payload = HashMap::with_capacity(3);
        payload.insert("content".to_string(), chunk.chunk.content.into());
        payload.insert(
            "start_char".to_string(),
            (chunk.chunk.position.start_char as i64).into(),
        );
        payload.insert(
            "end_char".to_string(),
            (chunk.chunk.position.end_char as i64).into(),
        );

        PointStruct {
            id: Some(point_id.into()),
            vectors: Some(Vectors {
                vectors_options: Some(qdrant_client::qdrant::vectors::VectorsOptions::Vector(
                    Vector {
                        data: chunk.embedding,
                        indices: None,
                        vector: None,
                        vectors_count: None,
                    },
                )),
            }),
            payload,
        }
    }
}

#[async_trait::async_trait]
impl StorageWriter for QdrantStore {
    type Input = Vec<EmbeddedChunk>;
    type Output = usize;
    async fn write(&self, input: Self::Input) -> anyhow::Result<Self::Output> {
        let points: Vec<PointStruct> = input.into_iter().map(From::from).collect();

        let points_count = points.len();
        let upsert_request = UpsertPoints {
            collection_name: self.collection_name.clone(),
            points,
            wait: Some(true), // Wait for the operation to complete
            ordering: None,
            shard_key_selector: None,
        };
        let _response = self.client.upsert_points(upsert_request).await?;
        // Return the number of successfully upserted points
        Ok(points_count)
    }
}
