use std::collections::HashMap;

use super::{
    ApiKey, AuthorizationCode, AuthorizationUrl, DeviceCode, OAuthConfig, PkceVerifier, State,
    URLParam, URLParamValue, UserCode, VerificationUri,
};

// ============================================================================
// API Key Flow
// ============================================================================

/// Request parameters for API key authentication
#[derive(Debug, Clone)]
pub struct ApiKeyRequest {
    pub required_params: Vec<URLParam>,
}

/// Response containing API key and URL parameters
#[derive(Debug, Clone)]
pub struct ApiKeyResponse {
    pub api_key: ApiKey,
    pub url_params: HashMap<URLParam, URLParamValue>,
}

// ============================================================================
// Authorization Code Flow
// ============================================================================

/// Authorization code OAuth authentication flow
#[derive(Debug, Clone)]
pub struct CodeAuthFlow;

/// Request parameters for authorization code flow
#[derive(Debug, Clone)]
pub struct CodeRequest {
    pub authorization_url: AuthorizationUrl,
    pub state: State,
    pub pkce_verifier: Option<PkceVerifier>,
    pub oauth_config: OAuthConfig,
}

/// Response containing authorization code, state and optional PKCE verifier
#[derive(Debug, Clone)]
pub struct CodeResponse {
    pub code: AuthorizationCode,
}

// ============================================================================
// Device Code Flow
// ============================================================================

/// Device code OAuth authentication flow
#[derive(Debug, Clone)]
pub struct DeviceCodeAuthFlow;

/// Request parameters for device code flow
#[derive(Debug, Clone)]
pub struct DeviceCodeRequest {
    pub user_code: UserCode,
    pub device_code: DeviceCode,
    pub verification_uri: VerificationUri,
    pub verification_uri_complete: Option<VerificationUri>,
    pub expires_in: u64,
    pub interval: u64,
    pub oauth_config: OAuthConfig,
}

/// Response containing device code and polling interval
#[derive(Debug, Clone)]
pub struct DeviceCodeResponse;

// ============================================================================
// Auth Context
// ============================================================================

#[derive(Debug, Clone)]
pub enum AuthContextRequest {
    ApiKey(ApiKeyRequest),
    DeviceCode(DeviceCodeRequest),
    Code(CodeRequest),
}

#[derive(Debug, Clone)]
pub struct FlowContext<Request, Response> {
    pub request: Request,
    pub response: Response,
}

#[derive(Debug, Clone)]
pub enum AuthContextResponse {
    ApiKey(FlowContext<ApiKeyRequest, ApiKeyResponse>),
    DeviceCode(FlowContext<DeviceCodeRequest, DeviceCodeResponse>),
    Code(FlowContext<CodeRequest, CodeResponse>),
}

impl AuthContextResponse {
    /// Creates an API key authentication context
    pub fn api_key(
        request: ApiKeyRequest,
        api_key: impl ToString,
        url_params: HashMap<String, String>,
    ) -> Self {
        Self::ApiKey(FlowContext {
            request,
            response: ApiKeyResponse {
                api_key: api_key.to_string().into(),
                url_params: url_params
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            },
        })
    }

    /// Creates a device code authentication context
    pub fn device_code(request: DeviceCodeRequest) -> Self {
        Self::DeviceCode(FlowContext { request, response: DeviceCodeResponse })
    }

    /// Creates an authorization code authentication context
    pub fn code(request: CodeRequest, code: impl ToString) -> Self {
        Self::Code(FlowContext {
            request,
            response: CodeResponse { code: code.to_string().into() },
        })
    }
}
