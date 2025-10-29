use super::{
    ApiKeyMethod, ApiKeyRequest, ApiKeyResponse, AuthMethod, AuthorizationCode, CodeMethod,
    CodeRequest, CodeResponse, DeviceCode, DeviceCodeMethod, DeviceCodeRequest, DeviceCodeResponse,
    PkceVerifier, State,
};

#[derive(Debug, Clone)]
pub enum AuthContextRequest {
    ApiKey(ApiKeyRequest),
    DeviceCode(DeviceCodeRequest),
    Code(CodeRequest),
}

#[derive(Debug, Clone)]
pub struct FlowContext<Request, Response, Method> {
    pub request: Request,
    pub response: Response,
    pub method: Method,
}

#[derive(Debug, Clone)]
pub enum AuthContextResponse {
    ApiKey(FlowContext<ApiKeyRequest, ApiKeyResponse, ApiKeyMethod>),
    DeviceCode(FlowContext<DeviceCodeRequest, DeviceCodeResponse, DeviceCodeMethod>),
    Code(FlowContext<CodeRequest, CodeResponse, CodeMethod>),
}

impl AuthContextResponse {
    /// Creates an API key authentication context
    pub fn api_key(request: ApiKeyRequest, response: ApiKeyResponse, method: ApiKeyMethod) -> Self {
        Self::ApiKey(FlowContext { request, response, method })
    }

    /// Creates a device code authentication context
    pub fn device_code(
        request: DeviceCodeRequest,
        response: DeviceCodeResponse,
        method: DeviceCodeMethod,
    ) -> Self {
        Self::DeviceCode(FlowContext { request, response, method })
    }

    /// Creates an authorization code authentication context
    pub fn code(request: CodeRequest, response: CodeResponse, method: CodeMethod) -> Self {
        Self::Code(FlowContext { request, response, method })
    }

    /// Extracts the method for this auth context
    ///
    /// # Returns
    /// Returns the runtime representation of the auth method
    pub fn method(&self) -> AuthMethod {
        match self {
            Self::ApiKey(_) => AuthMethod::ApiKey,
            Self::DeviceCode(ctx) => AuthMethod::OAuthDevice(ctx.method.oauth_config.clone()),
            Self::Code(ctx) => AuthMethod::OAuthCode(ctx.method.oauth_config.clone()),
        }
    }

    /// Extracts device code and interval from DeviceCode variant
    ///
    /// # Returns
    /// Returns `Some((device_code, interval))` if this is a DeviceCode flow,
    /// `None` otherwise
    pub fn as_device_code(&self) -> Option<(&DeviceCode, u64)> {
        match self {
            Self::DeviceCode(ctx) => Some((&ctx.request.device_code, ctx.request.interval)),
            _ => None,
        }
    }

    /// Extracts authorization code, state and PKCE verifier from Code variant
    ///
    /// # Returns
    /// Returns `Some((code, state, pkce_verifier))` if this is a Code flow,
    /// `None` otherwise
    pub fn as_code(&self) -> Option<(&AuthorizationCode, &State, Option<&PkceVerifier>)> {
        match self {
            Self::Code(ctx) => Some((
                &ctx.response.code,
                &ctx.response.state,
                ctx.response.pkce_verifier.as_ref(),
            )),
            _ => None,
        }
    }
}
