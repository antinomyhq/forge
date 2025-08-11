use forge_domain::ModelId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Model {
    pub id: ModelId,
    pub name: Option<String>,
    pub created: Option<u64>,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub architecture: Option<Architecture>,
    pub pricing: Option<Pricing>,
    pub top_provider: Option<TopProvider>,
    pub per_request_limits: Option<serde_json::Value>,
    pub supported_parameters: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Architecture {
    pub modality: String,
    pub tokenizer: String,
    pub instruct_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Pricing {
    pub prompt: Option<String>,
    pub completion: Option<String>,
    pub image: Option<String>,
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
    pub data: Vec<Model>,
}

impl From<Model> for forge_domain::Model {
    fn from(value: Model) -> Self {
        let tools_supported = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "tools");
        let supports_parallel_tool_calls = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "supports_parallel_tool_calls");
        let is_reasoning_supported = value
            .supported_parameters
            .iter()
            .flatten()
            .any(|param| param == "reasoning");

        forge_domain::Model {
            id: value.id,
            name: value.name,
            description: value.description,
            context_length: value.context_length,
            tools_supported: Some(tools_supported),
            supports_parallel_tool_calls: Some(supports_parallel_tool_calls),
            supports_reasoning: Some(is_reasoning_supported),
        }
    }
}
