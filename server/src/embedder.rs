use anyhow::{Context, Result};
use serde::Deserialize;

/// Maximum number of texts per single Ollama embedding request.
const EMBED_BATCH_SIZE: usize = 20;

/// Client for Ollama embedding API.
///
/// Generates vector embeddings using a locally-running Ollama instance.
/// Supports both single and batch embedding requests.
pub struct Embedder {
    client: reqwest::Client,
    url: String,
    model: String,
    dim: u64,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl Embedder {
    /// Creates a new Ollama embedding client.
    ///
    /// # Arguments
    /// * `ollama_url` - Base URL of the Ollama instance (e.g., `http://localhost:11434`)
    /// * `model` - Name of the embedding model (e.g., `nomic-embed-text`)
    /// * `dim` - Expected embedding dimension (e.g., 768)
    pub fn new(ollama_url: &str, model: &str, dim: u64) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
            url: ollama_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            dim,
        }
    }

    /// Embeds a single text string, returning its vector.
    pub async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let resp: EmbedResponse = self
            .client
            .post(format!("{}/api/embed", self.url))
            .json(&serde_json::json!({
                "model": self.model,
                "input": text,
            }))
            .send()
            .await
            .context("Failed to reach Ollama for embedding")?
            .error_for_status()
            .context("Ollama embedding request failed")?
            .json()
            .await
            .context("Failed to parse Ollama embedding response")?;

        let vec = resp
            .embeddings
            .into_iter()
            .next()
            .context("Ollama returned empty embeddings")?;

        if vec.len() != self.dim as usize {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dim,
                vec.len()
            );
        }

        Ok(vec)
    }

    /// Embeds multiple texts in a single batch request.
    ///
    /// Returns one vector per input text, in the same order.
    /// Splits inputs into chunks of `EMBED_BATCH_SIZE` to avoid overwhelming Ollama.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for batch in texts.chunks(EMBED_BATCH_SIZE) {
            let resp: EmbedResponse = self
                .client
                .post(format!("{}/api/embed", self.url))
                .json(&serde_json::json!({
                    "model": self.model,
                    "input": batch,
                }))
                .send()
                .await
                .context("Failed to reach Ollama for batch embedding")?
                .error_for_status()
                .context("Ollama batch embedding request failed")?
                .json()
                .await
                .context("Failed to parse Ollama batch embedding response")?;

            if resp.embeddings.len() != batch.len() {
                anyhow::bail!(
                    "Ollama returned {} embeddings for {} inputs",
                    resp.embeddings.len(),
                    batch.len()
                );
            }

            for (i, vec) in resp.embeddings.iter().enumerate() {
                if vec.len() != self.dim as usize {
                    anyhow::bail!(
                        "Embedding dimension mismatch at index {i}: expected {}, got {}",
                        self.dim,
                        vec.len()
                    );
                }
            }

            all_embeddings.extend(resp.embeddings);
        }

        Ok(all_embeddings)
    }

    /// Checks that Ollama is reachable.
    pub async fn health_check(&self) -> Result<()> {
        self.client
            .get(&self.url)
            .send()
            .await
            .context("Ollama is not reachable")?
            .error_for_status()
            .context("Ollama health check failed")?;
        Ok(())
    }
}
