use std::collections::HashMap;

use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use crate::{AccessToken, ApiKey, OAuthConfig, ProviderId, RefreshToken, URLParam, URLParamValue};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Setters)]
pub struct AuthCredential {
    id: ProviderId,
    auth_details: AuthDetails,
    url_params: Option<HashMap<URLParam, URLParamValue>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthDetails {
    ApiKey(ApiKey),
    OAuth {
        tokens: OAuthTokens,
        config: OAuthConfig,
    },
    OAuthWithApiKey {
        tokens: OAuthTokens,
        api_key: ApiKey,
        config: OAuthConfig,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: AccessToken,
    pub refresh_token: Option<RefreshToken>,
    pub expires_at: DateTime<Utc>,
}
