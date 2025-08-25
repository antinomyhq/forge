use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::{CreateEmbeddingRequest, EmbeddingInput};
use futures::future::join_all;

pub use crate::traits::Embedder;
use crate::transform::Transform;
use crate::{Chunk, EmbeddedChunk};

#[derive(Debug, Clone)]
pub struct ChunkEmbedder {
    model: String,
    client: Client<OpenAIConfig>,
    batch_size: usize,
}

impl ChunkEmbedder {
    pub fn new(model: String, batch_size: usize) -> Self {
        let client = Client::new();
        Self { model, client, batch_size }
    }
}

impl Transform for ChunkEmbedder {
    type In = Vec<Chunk>;
    type Out = Vec<EmbeddedChunk>;
    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let batches = input.chunks(self.batch_size).collect::<Vec<_>>();

        // Kick off all embedding requests in parallel
        let futures = batches.into_iter().map(|batch| {
            let client = self.client.clone();
            let model = self.model.clone();
            async move {
                let resp = client
                    .embeddings()
                    .create(CreateEmbeddingRequest {
                        model: model.clone(),
                        input: EmbeddingInput::StringArray(
                            batch
                                .iter()
                                .map(|inp| inp.content.clone())
                                .collect::<Vec<_>>(),
                        ),
                        ..Default::default()
                    })
                    .await?;

                anyhow::Ok(
                    resp.data
                        .into_iter()
                        .map(|e| e.embedding)
                        .collect::<Vec<Vec<f32>>>(),
                )
            }
        });

        // Await all the requests together
        let all_embeddings: Vec<Vec<Vec<f32>>> = join_all(futures)
            .await
            .into_iter()
            .collect::<anyhow::Result<_>>()?;

        // Flatten embeddings back into a single vec
        let embeddings_flat: Vec<Vec<f32>> = all_embeddings.into_iter().flatten().collect();
        embeddings_flat
            .into_iter()
            .zip(input.into_iter())
            .map(|(embedding, chunk)| {
                Ok(EmbeddedChunk { chunk, embedding, embedding_model: self.model.clone() })
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct QueryEmbedder {
    model: String,
    client: Client<OpenAIConfig>,
}

impl QueryEmbedder {
    pub fn new(model: String) -> Self {
        let client = Client::new();
        Self { model, client }
    }
}

impl Transform for QueryEmbedder {
    type In = String;
    type Out = Vec<f32>;
    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let embeddings = self
            .client
            .embeddings()
            .create(CreateEmbeddingRequest {
                model: self.model.clone(),
                input: EmbeddingInput::String(input),
                ..Default::default()
            })
            .await?;

        let embedding = embeddings
            .data
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Failed to generate embedding for query"))?;
        Ok(embedding.embedding)
    }
}
