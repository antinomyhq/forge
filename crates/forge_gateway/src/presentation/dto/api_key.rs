use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::ApiKey;

#[derive(Debug, Deserialize, Serialize, Validate)]
pub struct CreateApiKeyRequest {
    #[validate(length(min = 1, message = "name cannot be empty"))]
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub key_name: String,
    pub key: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl From<ApiKey> for ApiKeyResponse {
    fn from(api_key: ApiKey) -> Self {
        Self {
            id: api_key.id.to_string(),
            key_name: api_key.key_name,
            key: api_key.key,
            created_at: api_key.created_at,
            last_used_at: api_key.last_used_at,
        }
    }
}
