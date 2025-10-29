use std::collections::HashMap;

use super::{
    ApiKey, ApiKeyMethod, ApiKeyRequest, ApiKeyResponse, AuthMethod, CodeAuthFlow, CodeMethod,
    CodeRequest, CodeResponse, DeviceCode, DeviceCodeAuthFlow, DeviceCodeMethod, DeviceCodeRequest,
    DeviceCodeResponse, OAuthConfig, PkceVerifier, State, URLParam, URLParamValue,
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
            Self::DeviceCode(ctx) => Some((&ctx.response.device_code, ctx.response.interval)),
            _ => None,
        }
    }

    /// Extracts state and PKCE verifier from Code variant
    ///
    /// # Returns
    /// Returns `Some((state, pkce_verifier))` if this is a Code flow, `None`
    /// otherwise
    pub fn as_code(&self) -> Option<(&State, Option<&PkceVerifier>)> {
        match self {
            Self::Code(ctx) => Some((&ctx.response.state, ctx.response.pkce_verifier.as_ref())),
            _ => None,
        }
    }
}

impl AuthContextRequest {
    pub fn api_key(request: ApiKeyRequest, response: ApiKeyResponse, method: ApiKeyMethod) -> Self {
        todo!()
    }

    pub fn device_code(
        request: DeviceCodeRequest,
        response: DeviceCodeResponse,
        method: DeviceCodeMethod,
    ) -> Self {
        todo!()
    }

    pub fn code(request: CodeRequest, response: CodeResponse, method: CodeMethod) -> Self {
        todo!()
    }

    /// Extracts API key and URL parameters from ApiKey variant
    ///
    /// # Returns
    /// Returns `Some((api_key, url_params))` if this is an ApiKey flow, `None`
    /// otherwise
    pub fn as_api_key(&self) -> Option<(&ApiKey, &HashMap<URLParam, URLParamValue>)> {
        todo!()
    }

    /// Returns OAuth config if this flow uses OAuth
    ///
    /// # Returns
    /// Returns `Some(&OAuthConfig)` for OAuth flows, `None` for ApiKey flow
    pub fn oauth_config(&self) -> Option<&OAuthConfig> {
        todo!()
    }
}
