use derive_more::From;
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
    #[serde(default, skip_serializing_if = "is_default")]
    pub is_tracked: bool,
}

#[derive(Default, Clone, Serialize, Deserialize, From)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    #[serde(default, skip_serializing_if = "is_default")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub api_key_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub api_key_masked: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub auth_provider_id: Option<String>,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}
