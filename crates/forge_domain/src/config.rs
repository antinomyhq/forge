use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeConfig {
    pub key_info: Option<ForgeKey>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeKey {
    #[serde(rename = "apiKey")]
    pub key: String,
    #[serde(rename = "keyName")]
    pub name: Option<String>,
}
