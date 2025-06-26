use derive_more::From;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitAuth {
    pub session_id: String,
    pub auth_url: String,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeConfig {
    pub key_info: Option<LoginInfo>,
}

#[derive(Clone, Serialize, Deserialize, From)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    pub api_key: String,
    pub key_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub email: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
