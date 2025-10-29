use super::{AuthorizationUrl, OAuthConfig, PkceVerifier, State};

/// Authorization code OAuth authentication flow
#[derive(Debug, Clone)]
pub struct CodeAuthFlow;

/// Request parameters for authorization code flow
#[derive(Debug, Clone)]
pub struct CodeRequest {
    pub authorization_url: AuthorizationUrl,
    pub state: State,
}

/// Response containing state and optional PKCE verifier
#[derive(Debug, Clone)]
pub struct CodeResponse {
    pub state: State,
    pub pkce_verifier: Option<PkceVerifier>,
}

/// Method configuration for authorization code flow
#[derive(Debug, Clone)]
pub struct CodeMethod {
    pub oauth_config: OAuthConfig,
}
