use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ApiKey, UserId};

/// Response from initializing an authentication flow
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitFlowResponse {
    /// 6-character alphanumeric device/session ID
    pub device_id: String,
    /// Time-to-live in seconds for the session
    pub ttl: u64,
    /// Base64-encoded initialization vector
    pub iv: String,
    /// Additional authenticated data
    pub aad: String,
}

impl InitFlowResponse {
    /// Create a new init flow response
    pub fn new(device_id: String, ttl: u64, iv: String, aad: String) -> Self {
        Self { device_id, ttl, iv, aad }
    }
}

/// Login information from authentication flow polling
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthFlowLoginInfo {
    /// The authentication token (full API key)
    pub token: ApiKey,
    /// Masked version of the token for display purposes
    pub masked_token: String,
    /// User identifier
    pub user_id: UserId,
}

impl AuthFlowLoginInfo {
    /// Create a new login info
    pub fn new(token: ApiKey, masked_token: String, user_id: UserId) -> Self {
        Self { token, masked_token, user_id }
    }
}

/// Information about an API key
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    /// Unique identifier for the API key (UUID)
    pub id: String,
    /// Masked version of the API key for display purposes
    pub masked_token: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl ApiKeyInfo {
    /// Create a new API key info entry
    pub fn new(id: String, masked_token: String, created_at: DateTime<Utc>) -> Self {
        Self { id, masked_token, created_at }
    }
}
