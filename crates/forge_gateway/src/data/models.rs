use std::fmt::Display;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiKeyId(Uuid);

impl Display for ApiKeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ApiKeyId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiKey {
    pub id: ApiKeyId,
    pub user_id: String,
    pub key_name: String,
    pub key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NewApiKey {
    pub user_id: String,
    pub key_name: String,
    pub key: String,
}

impl From<NewApiKey> for String {
    fn from(val: NewApiKey) -> Self {
        serde_json::to_string(&val).unwrap()
    }
}

impl NewApiKey {
    pub fn new(user_id: String, key_name: String, key: String) -> Self {
        Self { user_id, key_name, key }
    }
}
