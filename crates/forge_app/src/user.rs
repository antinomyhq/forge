use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthProviderId(String);

impl AuthProviderId {
    pub fn new(id: impl ToString) -> Self {
        Self(id.to_string())
    }
    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub auth_provider_id: AuthProviderId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageInfo {
    pub current: u32,
    pub limit: u32,
    pub remaining: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_in: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserUsage {
    pub plan: Plan,
    pub usage: UsageInfo,
}
