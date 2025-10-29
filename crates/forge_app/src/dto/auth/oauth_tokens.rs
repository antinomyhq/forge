use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::dto::{AccessToken, RefreshToken};

/// OAuth tokens for providers using OAuth authentication
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct OAuthTokens {
    /// Long-lived token for getting new access tokens
    pub refresh_token: RefreshToken,

    /// Short-lived token for API requests
    pub access_token: AccessToken,

    /// When the access token expires
    pub expires_at: DateTime<Utc>,
}

impl OAuthTokens {
    /// Creates new OAuth tokens
    pub fn new(
        refresh_token: impl Into<RefreshToken>,
        access_token: impl Into<AccessToken>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            refresh_token: refresh_token.into(),
            access_token: access_token.into(),
            expires_at,
        }
    }

    /// Checks if token will expire within the given duration
    pub fn expires_within(&self, seconds: i64) -> bool {
        let threshold = Utc::now() + chrono::Duration::seconds(seconds);
        self.expires_at <= threshold
    }
}
