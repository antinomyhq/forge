use async_openai::config::OpenAIConfig;
use async_openai::types::{CreateEmbeddingRequest, EmbeddingInput};
use tracing::info;

use super::{Embedder, EmbedderInput, EmbedderOutput};
use crate::token_counter::TokenCounter;

/// Maximum tokens supported by OpenAI embedding batch endpoint.
const TOKEN_LIMIT_PER_BATCH: usize = 30_000;

#[derive(Clone)]
pub struct OpenAI {
    model: String,
    dims: u32,
    base_url: Option<String>,
    api_key: Option<String>,
}

impl OpenAI {
    pub fn new<S: Into<String>>(embedding_model: S, dims: u32) -> Self {
        Self {
            model: embedding_model.into(),
            dims,
            base_url: None,
            api_key: None,
        }
    }

    /// Sets a custom base URL for the OpenAI API
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Sets the API key for OpenAI API authentication
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    fn openai_config(&self) -> OpenAIConfig {
        let mut config = async_openai::config::OpenAIConfig::new();
        if let Some(api_key) = self.api_key.as_ref() {
            config = config.with_api_key(api_key);
        }
        if let Some(base_url) = self.base_url.as_ref() {
            config = config.with_api_base(base_url);
        }
        config
    }

    /// Process a single batch of texts to get embeddings
    async fn process_batch(
        &self,
        batch: Vec<String>,
        model: &str,
        dims: u32,
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        let client = async_openai::Client::with_config(self.openai_config());

        let result = client
            .embeddings()
            .create(CreateEmbeddingRequest {
                model: model.into(),
                input: EmbeddingInput::StringArray(batch),
                encoding_format: Some(async_openai::types::EncodingFormat::Float),
                dimensions: Some(dims),
                user: None,
            })
            .await?;

        Ok(result.data.into_iter().map(|d| d.embedding).collect())
    }
}

#[async_trait::async_trait]
impl Embedder for OpenAI {
    async fn embed<T>(&self, inputs: Vec<EmbedderInput<T>>) -> anyhow::Result<Vec<EmbedderOutput>>
    where
        T: ToString + Send,
    {
        info!("Embedding {} blocks with OpenAI", inputs.len());

        // Process batches of texts
        let texts = inputs
            .iter()
            .map(|p| p.payload.to_string())
            .collect::<Vec<_>>();

        // Pre-allocate with the exact capacity we need
        let mut embeddings = Vec::with_capacity(texts.len());

        // 30,000 is limit of OpenAI embedding API.
        let batches =
            TokenAwareBatcher::try_new(&self.model, TOKEN_LIMIT_PER_BATCH)?.create_batches(texts);
        for batch in batches {
            let batch_results = self.process_batch(batch, &self.model, self.dims).await?;
            embeddings.extend(batch_results);
        }

        // Convert to EmbedderOutput format
        let results = embeddings
            .into_iter()
            .map(|embeddings| EmbedderOutput { embeddings })
            .collect();

        Ok(results)
    }
}

/// A token-aware batching utility that groups texts while respecting token
/// limits.
pub struct TokenAwareBatcher {
    /// Maximum tokens allowed per batch
    pub max_tokens_per_batch: usize,
    pub tokenizer: TokenCounter,
}

impl TokenAwareBatcher {
    pub fn try_new(model: &str, max_tokens_per_batch: usize) -> anyhow::Result<Self> {
        Ok(Self {
            max_tokens_per_batch,
            tokenizer: TokenCounter::try_from(model)?,
        })
    }
}

impl TokenAwareBatcher {
    pub fn estimate_tokens(&self, text: &str) -> usize {
        self.tokenizer.count_tokens(text)
    }

    /// Creates optimally-sized batches from input texts while respecting token
    /// limits.
    ///
    /// The batching strategy:
    /// - Accumulates texts into batches until reaching token limit
    /// - Handles oversized texts by processing them in individual batches
    /// - Ensures no batch exceeds the configured token limit
    pub fn create_batches(&self, texts: Vec<String>) -> Vec<Vec<String>> {
        let mut batches = Vec::new();
        let mut current_batch = Vec::new();
        let mut current_batch_tokens = 0;

        for text in texts {
            let token_estimate = self.estimate_tokens(&text);

            // If a single text exceeds the limit, we could either:
            // 1. Skip it (bad for completeness)
            // 2. Truncate it (bad for meaning)
            // 3. Process it alone (may fail but gives API a chance to handle it)
            // We choose option 3 here
            if token_estimate > self.max_tokens_per_batch {
                if !current_batch.is_empty() {
                    batches.push(std::mem::take(&mut current_batch));
                    current_batch_tokens = 0;
                }
                batches.push(vec![text]);
                continue;
            }

            // If adding this would exceed the batch limit, finalize current batch
            if current_batch_tokens + token_estimate > self.max_tokens_per_batch
                && !current_batch.is_empty()
            {
                batches.push(std::mem::take(&mut current_batch));
                current_batch_tokens = 0;
            }

            // Add to current batch
            current_batch_tokens += token_estimate;
            current_batch.push(text);
        }

        // Don't forget the last batch if it has items
        if !current_batch.is_empty() {
            batches.push(current_batch);
        }

        batches
    }
}
