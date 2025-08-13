use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::{CreateEmbeddingRequest, EmbeddingInput};

pub use crate::traits::Embedder;
use crate::{Chunk, EmbeddedChunk};

#[derive(Debug, Clone)]
pub struct ChunkEmbedder {
    model: String,
    client: Client<OpenAIConfig>,
}

impl ChunkEmbedder {
    pub fn new(model: String) -> Self {
        let client = Client::new();
        Self { model, client }
    }
}

#[async_trait::async_trait]
impl Embedder for ChunkEmbedder {
    type Input = Vec<Chunk>;
    type Output = Vec<EmbeddedChunk>;

    async fn embed(&self, inputs: Self::Input) -> anyhow::Result<Self::Output> {
        let embeddings = self
            .client
            .embeddings()
            .create(CreateEmbeddingRequest {
                model: self.model.clone(),
                input: EmbeddingInput::StringArray(
                    inputs
                        .iter()
                        .map(|inp| inp.content.clone())
                        .collect::<Vec<_>>(),
                ),
                ..Default::default()
            })
            .await?;

        embeddings
            .data
            .into_iter()
            .zip(inputs.into_iter())
            .map(|(embedding, chunk)| {
                Ok(EmbeddedChunk {
                    chunk,
                    embedding: embedding.embedding,
                    embedding_model: self.model.clone(),
                })
            })
            .collect()
    }
}
