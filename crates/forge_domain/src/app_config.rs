use std::collections::HashMap;

use derive_more::From;
use serde::{Deserialize, Serialize};

use crate::{AgentId, ModelId, ProviderId};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitAuth {
    pub session_id: String,
    pub auth_url: String,
    pub token: String,
}

/// Per-agent model override storing both provider and model.
///
/// Model IDs are scoped to a specific provider's namespace, so both must be
/// stored together to correctly resolve the model during agent loading.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelConfig {
    /// The provider this model belongs to.
    pub provider: ProviderId,
    /// The model ID within the provider's namespace.
    pub model: ModelId,
}

#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub key_info: Option<LoginInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub model: HashMap<ProviderId, ModelId>,
    /// Per-agent model overrides. When set, takes precedence over the agent's
    /// YAML definition and the global default model.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub agent_model: HashMap<AgentId, AgentModelConfig>,
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
