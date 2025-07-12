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
