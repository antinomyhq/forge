use anyhow::Context;
use forge_domain::Environment;
use forge_services::EmbeddingService;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

pub struct OpenAIEmbeddingService {
    client: reqwest::Client,
    env: Environment,
}

impl OpenAIEmbeddingService {
    pub const EMBEDDING_MODEL: &str = "text-embedding-ada-002";
    pub fn new(env: Environment) -> Self {
        let client = reqwest::Client::new();
        Self { client, env }
    }
}

#[async_trait::async_trait]
impl EmbeddingService for OpenAIEmbeddingService {
    async fn embed(&self, sentence: &str) -> anyhow::Result<Vec<f32>> {
        let mut headers = HeaderMap::new();
        let api_key = self
            .env
            .openai_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OpenAI API key is not set"))?;
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .context("Failed to create auth header")?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let request = EmbeddingRequest {
            model: Self::EMBEDDING_MODEL.to_string(),
            input: sentence.to_string(),
        };

        let response: EmbeddingResponse = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .headers(headers)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI")?
            .error_for_status()?
            .json()
            .await
            .context("Failed to parse OpenAI response")?;

        let embeddings = response
            .data
            .into_iter()
            .flat_map(|data| data.embedding)
            .collect();

        Ok(embeddings)
    }
}
