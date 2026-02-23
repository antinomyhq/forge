use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct ClientId(String);

/// Redirect URIs for post-authentication callbacks.
///
/// After the OAuth flow completes via the local redirect URI, the user can be
/// redirected to a success or failure URL to provide visual feedback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallbackRedirect {
    pub success_uri: String,
    pub failure_uri: String,
}

/// OAuth configuration for authentication flows
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub auth_url: Url,
    pub token_url: Url,
    pub client_id: ClientId,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_redirect_uris: Option<Vec<String>>,
    #[serde(default)]
    pub use_pkce: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_refresh_url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_auth_params: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_redirect: Option<CallbackRedirect>,
}
