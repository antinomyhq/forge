use derive_more::From;
use derive_setters::Setters;
use forge_domain::{AgentId, ModelId};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitAuth {
    pub session_id: String,
    pub auth_url: String,
    pub token: String,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub key_info: Option<LoginInfo>,
}

#[derive(Clone, Serialize, Deserialize, From)]
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

#[derive(Default, Clone, Serialize, Deserialize, Setters)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
pub struct WorkspaceConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operating_agent: Option<AgentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model: Option<ModelId>,
}
