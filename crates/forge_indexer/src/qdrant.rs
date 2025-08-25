use std::collections::HashMap;

use derive_setters::Setters;
use qdrant_client::Qdrant;
use qdrant_client::config::QdrantConfig;
use qdrant_client::qdrant::vectors_config::Config;
use qdrant_client::qdrant::{
    CreateCollection, Distance, PointStruct, SearchPoints, UpsertPoints, Vector, VectorParams,
    Vectors, VectorsConfig, WithPayloadSelector,
};
use uuid::Uuid;

use crate::EmbeddedChunk;
use crate::transform::Transform;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub content: String,
    pub score: f32,
    pub start_char: usize,
    pub end_char: usize,
}

#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub embedding: Vec<f32>,
    pub limit: u64,
    pub score_threshold: Option<f32>,
}

pub struct QdrantStore {
    client: Qdrant,
    collection_name: String,
}

impl QdrantStore {
    pub fn try_new(api_key: String, url: String, collection_name: String) -> anyhow::Result<Self> {
        let client = QdrantConfig::from_url(&url).api_key(api_key).build()?;
        Ok(Self { client, collection_name })
    }

    pub async fn delete_collection(&self) -> anyhow::Result<()> {
        self.client.delete_collection(&self.collection_name).await?;
        Ok(())
    }
    async fn ensure_collection_exists(&self, dims: u64) -> anyhow::Result<()> {
        // First check if collection exists
        match self.client.collection_exists(&self.collection_name).await {
            Ok(exists) if exists => Ok(()),
            Ok(_) => {
                // Collection doesn't exist, attempt to create it
                let create_collection = CreateCollection {
                    collection_name: self.collection_name.to_string(),
                    vectors_config: Some(VectorsConfig {
                        config: Some(Config::Params(VectorParams {
                            size: dims,
                            distance: Distance::Cosine.into(),
                            ..Default::default()
                        })),
                    }),
                    ..Default::default()
                };

                // Attempt to create collection - if another thread already created it,
                // this will fail gracefully and we can continue
                match self.client.create_collection(create_collection).await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        // Check if the error is due to collection already existing
                        // In Qdrant, creating an existing collection returns an error
                        // but we can verify by checking if it exists again
                        if self.client.collection_exists(&self.collection_name).await? {
                            Ok(())
                        } else {
                            Err(e.into())
                        }
                    }
                }
            }
            Err(e) => Err(e.into()),
        }
    }
}

impl From<EmbeddedChunk> for PointStruct {
    fn from(chunk: EmbeddedChunk) -> Self {
        // Create a unique ID for each point using UUID
        let point_id = Uuid::new_v4().to_string();

        // Create payload with chunk metadata
        let mut payload = HashMap::with_capacity(4);
        payload.insert("content".to_string(), chunk.chunk.content.into());
        payload.insert(
            "start_char".to_string(),
            (chunk.chunk.position.start_char as i64).into(),
        );
        payload.insert(
            "end_char".to_string(),
            (chunk.chunk.position.end_char as i64).into(),
        );
        payload.insert(
            "path".to_string(),
            chunk.chunk.path.display().to_string().into(),
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

impl Transform for QdrantStore {
    type In = Vec<EmbeddedChunk>;
    type Out = usize;

    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let dims = input[0].embedding.len() as u64;
        let points: Vec<PointStruct> = input.into_iter().map(From::from).collect();

        // Ensure collection exists - safe for parallel environments
        self.ensure_collection_exists(dims).await?;

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

#[derive(Setters)]
pub struct QdrantRetriever {
    client: Qdrant,
    collection_name: String,
}

impl QdrantRetriever {
    pub fn try_new(api_key: String, url: String, collection_name: String) -> anyhow::Result<Self> {
        let client = QdrantConfig::from_url(&url).api_key(api_key).build()?;
        Ok(Self { client, collection_name })
    }
}

#[derive(Setters)]
#[setters(strip_option)]
pub struct RetrivalRequest {
    pub limit: u64,
    pub score_threshold: Option<f32>,
    pub embedding: Vec<f32>,
}

impl RetrivalRequest {
    pub fn new(embedding: Vec<f32>, limit: u64) -> Self {
        Self { limit, score_threshold: None, embedding }
    }
}

impl Transform for QdrantRetriever {
    type In = RetrivalRequest;
    type Out = Vec<SearchResult>;

    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let search_request = SearchPoints {
            collection_name: self.collection_name.clone(),
            vector: input.embedding,
            vector_name: None,
            limit: input.limit,
            score_threshold: input.score_threshold,
            with_payload: Some(WithPayloadSelector {
                selector_options: Some(
                    qdrant_client::qdrant::with_payload_selector::SelectorOptions::Enable(true),
                ),
            }),
            filter: None,
            params: None,
            offset: None,
            with_vectors: None,
            read_consistency: None,
            shard_key_selector: None,
            sparse_indices: None,
            timeout: None,
        };

        let search_result = self.client.search_points(search_request).await?;

        let results: anyhow::Result<Vec<SearchResult>> = search_result
            .result
            .into_iter()
            .map(|scored_point| {
                let payload = scored_point.payload;

                let content = payload
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing content in payload"))?
                    .to_string();

                let start_char = payload
                    .get("start_char")
                    .and_then(|v| v.as_integer())
                    .ok_or_else(|| anyhow::anyhow!("Missing start_char in payload"))?
                    as usize;

                let end_char = payload
                    .get("end_char")
                    .and_then(|v| v.as_integer())
                    .ok_or_else(|| anyhow::anyhow!("Missing end_char in payload"))?
                    as usize;

                let path = payload
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing path in payload"))?
                    .to_string();

                Ok(SearchResult {
                    content,
                    score: scored_point.score,
                    start_char,
                    end_char,
                    path,
                })
            })
            .collect();

        results
    }
}
