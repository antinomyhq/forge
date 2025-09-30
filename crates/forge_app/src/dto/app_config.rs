use derive_more::From;
use forge_domain::AgentId;
use serde::{Deserialize, Serialize};

use crate::dto::ProviderId;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitAuth {
    pub session_id: String,
    pub auth_url: String,
    pub token: String,
}

#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub key_info: Option<LoginInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_agent: Option<AgentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider: Option<ProviderId>,
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
