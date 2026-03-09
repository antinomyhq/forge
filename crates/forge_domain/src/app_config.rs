use std::collections::HashMap;

use derive_more::From;
use serde::{Deserialize, Serialize};

use crate::{ModelId, ProviderId, ReasoningConfig};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitAuth {
    pub session_id: String,
    pub auth_url: String,
    pub token: String,
}

/// Per-model configuration that can be set at runtime.
#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct ModelConfig {
    /// Reasoning configuration for this specific model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}

#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub key_info: Option<LoginInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub model: HashMap<ProviderId, ModelId>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub provider_config: HashMap<ProviderId, HashMap<ModelId, ModelConfig>>,
}

#[derive(Clone, Serialize, Deserialize, From, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    pub api_key: String,
    pub api_key_name: String,
    pub api_key_masked: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_provider_id: Option<String>,
}
