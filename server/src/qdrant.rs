use anyhow::{Context, Result};
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    Condition, CreateCollectionBuilder, DeletePointsBuilder, Distance, Filter,
    PointStruct, SearchPointsBuilder, VectorParamsBuilder,
    Value, UpsertPointsBuilder,
};

const DELETE_BATCH_SIZE: usize = 100;

/// A point to be upserted into Qdrant, representing a single file chunk.
pub struct ChunkPoint {
    /// Unique point ID (UUID string)
    pub id: String,
    /// Embedding vector
    pub vector: Vec<f32>,
    /// Source file path
    pub file_path: String,
    /// Chunk text content
    pub content: String,
    /// Start line in source file (1-based)
    pub start_line: u32,
    /// End line in source file (1-based, inclusive)
    pub end_line: u32,
}

/// Search result from Qdrant.
pub struct SearchHit {
    /// Point ID
    pub id: String,
    /// Cosine similarity score (0..1)
    pub score: f32,
    /// Source file path
    pub file_path: String,
    /// Chunk text content
    pub content: String,
    /// Start line in source file
    pub start_line: u32,
    /// End line in source file
    pub end_line: u32,
}

/// Wrapper around the Qdrant client for workspace vector operations.
///
/// Each workspace maps to a Qdrant collection named `ws_{workspace_id}`.
pub struct QdrantStore {
    client: Qdrant,
    embedding_dim: u64,
}

impl QdrantStore {
    /// Creates a new Qdrant store.
    ///
    /// # Arguments
    /// * `qdrant_url` - Qdrant gRPC endpoint (e.g., `http://localhost:6334`)
    /// * `embedding_dim` - Vector dimension (must match the embedding model)
    pub async fn new(qdrant_url: &str, embedding_dim: u64) -> Result<Self> {
        let client = Qdrant::from_url(qdrant_url)
            .build()
            .context("Failed to create Qdrant client")?;
        Ok(Self { client, embedding_dim })
    }

    /// Returns the collection name for a workspace.
    fn collection_name(workspace_id: &str) -> String {
        format!("ws_{workspace_id}")
    }

    /// Creates the Qdrant collection for a workspace if it doesn't exist.
    pub async fn ensure_collection(&self, workspace_id: &str) -> Result<()> {
        let name = Self::collection_name(workspace_id);

        let exists = self
            .client
            .collection_exists(&name)
            .await
            .context("Failed to check if Qdrant collection exists")?;

        if !exists {
            self.client
                .create_collection(
                    CreateCollectionBuilder::new(&name)
                        .vectors_config(VectorParamsBuilder::new(self.embedding_dim, Distance::Cosine)),
                )
                .await
                .with_context(|| format!("Failed to create Qdrant collection '{name}'"))?;
        }

        Ok(())
    }

    /// Upserts chunk points into a workspace's collection.
    ///
    /// Returns the list of point IDs that were upserted.
    pub async fn upsert_chunks(
        &self,
        workspace_id: &str,
        chunks: Vec<ChunkPoint>,
    ) -> Result<Vec<String>> {
        if chunks.is_empty() {
            return Ok(vec![]);
        }

        let collection = Self::collection_name(workspace_id);
        let mut ids = Vec::with_capacity(chunks.len());

        let points: Vec<PointStruct> = chunks
            .into_iter()
            .map(|chunk| {
                ids.push(chunk.id.clone());
                let mut payload = std::collections::HashMap::new();
                payload.insert("file_path".to_string(), Value::from(chunk.file_path));
                payload.insert("content".to_string(), Value::from(chunk.content));
                payload.insert("start_line".to_string(), Value::from(chunk.start_line as i64));
                payload.insert("end_line".to_string(), Value::from(chunk.end_line as i64));
                payload.insert("node_kind".to_string(), Value::from("file_chunk"));

                PointStruct::new(chunk.id, chunk.vector, payload)
            })
            .collect();

        self.client
            .upsert_points(UpsertPointsBuilder::new(&collection, points).wait(true))
            .await
            .context("Failed to upsert points into Qdrant")?;

        Ok(ids)
    }

    /// Deletes all points matching any of the given file paths.
    ///
    /// Batches the delete operations by `DELETE_BATCH_SIZE` paths at a time
    /// instead of one giant OR filter.
    /// Returns the number of file paths processed (not exact point count).
    pub async fn delete_by_file_paths(
        &self,
        workspace_id: &str,
        paths: &[String],
    ) -> Result<u32> {
        if paths.is_empty() {
            return Ok(0);
        }

        let collection = Self::collection_name(workspace_id);

        for batch in paths.chunks(DELETE_BATCH_SIZE) {
            let filter = Filter::any(
                batch
                    .iter()
                    .map(|p| Condition::matches("file_path", p.clone()))
                    .collect::<Vec<_>>(),
            );

            self.client
                .delete_points(
                    DeletePointsBuilder::new(&collection)
                        .points(filter)
                        .wait(true),
                )
                .await
                .context("Failed to delete points from Qdrant")?;
        }

        Ok(paths.len() as u32)
    }

    /// Performs ANN search in a workspace collection.
    ///
    /// # Arguments
    /// * `workspace_id` - Target workspace
    /// * `vector` - Query embedding vector
    /// * `limit` - Maximum results to return
    /// * `starts_with` - Optional file path prefix filters
    /// * `ends_with` - Optional file extension suffix filters
    pub async fn search(
        &self,
        workspace_id: &str,
        vector: Vec<f32>,
        limit: u32,
        starts_with: &[String],
        ends_with: &[String],
    ) -> Result<Vec<SearchHit>> {
        let collection = Self::collection_name(workspace_id);

        let mut conditions: Vec<Condition> = Vec::new();

        // File path prefix filter (exact keyword match)
        for prefix in starts_with {
            conditions.push(Condition::matches("file_path", prefix.clone()));
        }

        // File extension suffix filter
        // Qdrant doesn't have native "ends_with". We use a full-text match
        // condition on the file_path field. This works for extension filters
        // like ".rs" because Qdrant tokenizes on "." and "/" for keyword fields.
        // For more precise filtering, file_extension should be stored as a
        // separate payload field.
        for suffix in ends_with {
            conditions.push(Condition::matches("file_path", suffix.clone()));
        }

        let filter = if conditions.is_empty() {
            None
        } else {
            Some(Filter::all(conditions))
        };

        let mut search_builder = SearchPointsBuilder::new(&collection, vector, limit as u64)
            .with_payload(true);

        if let Some(f) = filter {
            search_builder = search_builder.filter(f);
        }

        let results = self
            .client
            .search_points(search_builder)
            .await
            .context("Failed to search Qdrant")?;

        let hits = results
            .result
            .into_iter()
            .filter_map(|point| {
                let payload = point.payload;
                let point_id = point.id?;
                let id = match point_id.point_id_options? {
                    qdrant_client::qdrant::point_id::PointIdOptions::Uuid(u) => u,
                    qdrant_client::qdrant::point_id::PointIdOptions::Num(n) => n.to_string(),
                };
                let file_path = payload.get("file_path")?.as_str()?.to_string();
                let content = payload.get("content")?.as_str()?.to_string();
                let start_line = payload.get("start_line")?.as_integer()? as u32;
                let end_line = payload.get("end_line")?.as_integer()? as u32;

                Some(SearchHit {
                    id,
                    score: point.score,
                    file_path,
                    content,
                    start_line,
                    end_line,
                })
            })
            .collect();

        Ok(hits)
    }

    /// Deletes the entire collection for a workspace.
    pub async fn delete_collection(&self, workspace_id: &str) -> Result<()> {
        let name = Self::collection_name(workspace_id);
        let exists = self
            .client
            .collection_exists(&name)
            .await
            .context("Failed to check Qdrant collection existence")?;

        if exists {
            self.client
                .delete_collection(&name)
                .await
                .with_context(|| format!("Failed to delete Qdrant collection '{name}'"))?;
        }

        Ok(())
    }
}
