use derive_setters::Setters;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::Transform;

#[derive(Debug, Clone)]
pub struct VoyageReRanker {
    api_key: String,
    client: Client,
}

impl VoyageReRanker {
    pub fn new(api_key: String) -> Self {
        Self { api_key, client: Client::new() }
    }
}

#[derive(Debug, Serialize, Setters)]
#[setters(strip_option)]
pub struct Request {
    pub query: String,
    pub documents: Vec<String>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_documents: Option<bool>,
}

impl Request {
    pub fn new(query: String, docs: Vec<String>, model: String) -> Self {
        Self {
            query,
            documents: docs,
            model,
            top_k: None,
            return_documents: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Response {
    object: String,
    pub data: Vec<Document>,
    model: String,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    total_tokens: usize,
}

#[derive(Debug, Deserialize)]
pub struct Document {
    pub relevance_score: f32,
    pub index: usize,
    pub document: Option<String>,
}

impl Transform for VoyageReRanker {
    type In = Request;
    type Out = Response;
    async fn transform(self, input: Self::In) -> anyhow::Result<Self::Out> {
        let request = self
            .client
            .request(reqwest::Method::POST, "https://api.voyageai.com/v1/rerank")
            .bearer_auth(&self.api_key)
            .json(&input)
            .build()?;

        let response = self
            .client
            .execute(request)
            .await?
            .json::<Response>()
            .await?;
        Ok(response)
    }
}
