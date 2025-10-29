use std::collections::HashMap;

use super::{
    ApiKey, ApiKeyAuthFlow, ApiKeyMethod, ApiKeyRequest, ApiKeyResponse, AuthMethod, CodeAuthFlow,
    CodeMethod, CodeRequest, CodeResponse, DeviceCode, DeviceCodeAuthFlow, DeviceCodeMethod,
    DeviceCodeRequest, DeviceCodeResponse, FlowContext, OAuthConfig, PkceVerifier, State, URLParam,
    URLParamValue,
};

/// Runtime polymorphic authentication flow context
#[derive(Debug, Clone)]
pub enum AuthContext {
    ApiKey(FlowContext<ApiKeyAuthFlow>),
    DeviceCode(FlowContext<DeviceCodeAuthFlow>),
    Code(FlowContext<CodeAuthFlow>),
}

impl AuthContext {
    pub fn api_key(request: ApiKeyRequest, response: ApiKeyResponse, method: ApiKeyMethod) -> Self {
        Self::ApiKey(FlowContext::new(request, response, method))
    }

    pub fn device_code(
        request: DeviceCodeRequest,
        response: DeviceCodeResponse,
        method: DeviceCodeMethod,
    ) -> Self {
        Self::DeviceCode(FlowContext::new(request, response, method))
    }

    pub fn code(request: CodeRequest, response: CodeResponse, method: CodeMethod) -> Self {
        Self::Code(FlowContext::new(request, response, method))
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

    /// Extracts API key and URL parameters from ApiKey variant
    ///
    /// # Returns
    /// Returns `Some((api_key, url_params))` if this is an ApiKey flow, `None`
    /// otherwise
    pub fn as_api_key(&self) -> Option<(&ApiKey, &HashMap<URLParam, URLParamValue>)> {
        match self {
            Self::ApiKey(ctx) => Some((&ctx.response.api_key, &ctx.response.url_params)),
            _ => None,
        }
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

    /// Returns OAuth config if this flow uses OAuth
    ///
    /// # Returns
    /// Returns `Some(&OAuthConfig)` for OAuth flows, `None` for ApiKey flow
    pub fn oauth_config(&self) -> Option<&OAuthConfig> {
        match self {
            Self::DeviceCode(ctx) => Some(&ctx.method.oauth_config),
            Self::Code(ctx) => Some(&ctx.method.oauth_config),
            Self::ApiKey(_) => None,
        }
    }
}
