use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    AccessToken, ApiKey, AuthorizationCode, PkceVerifier, RefreshToken, State, URLParam,
    URLParamValue,
};

/// Result of an authentication flow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResult {
    ApiKey {
        api_key: ApiKey,
        url_params: HashMap<URLParam, URLParamValue>,
    },
    OAuthTokens {
        access_token: AccessToken,
        refresh_token: Option<RefreshToken>,
        expires_in: Option<u64>,
    },
    AuthorizationCode {
        code: AuthorizationCode,
        state: State,
        code_verifier: Option<PkceVerifier>,
    },
}
