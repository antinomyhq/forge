use forge_domain::ModelId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenRouterModel {
    pub id: ModelId,
    pub name: Option<String>,
    pub created: Option<u64>,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub architecture: Option<Architecture>,
    pub pricing: Option<Pricing>,
    pub top_provider: Option<TopProvider>,
    pub per_request_limits: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Architecture {
    pub modality: String,
    pub tokenizer: String,
    pub instruct_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Pricing {
    pub prompt: String,
    pub completion: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TopProvider {
    pub context_length: Option<u64>,
    pub max_completion_tokens: Option<u64>,
    pub is_moderated: bool,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ListModelResponse {
    pub data: Vec<OpenRouterModel>,
}
