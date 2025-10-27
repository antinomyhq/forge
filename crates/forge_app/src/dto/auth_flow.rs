use std::collections::HashMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub use_pkce: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_refresh_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_auth_params: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    ApiKey,
    #[serde(rename = "oauth_device")]
    OAuthDevice(OAuthConfig),
    #[serde(rename = "oauth_code")]
    OAuthCode(OAuthConfig),
}

impl AuthMethod {
    pub fn oauth_device(config: OAuthConfig) -> Self {
        Self::OAuthDevice(config)
    }

    pub fn oauth_code(config: OAuthConfig) -> Self {
        Self::OAuthCode(config)
    }

    pub fn oauth_config(&self) -> Option<&OAuthConfig> {
        match self {
            Self::OAuthDevice(config) | Self::OAuthCode(config) => Some(config),
            Self::ApiKey => None,
        }
    }
}

/// Trait for type-safe authentication flows
///
/// Ensures request and response types are correctly paired at compile time
pub trait AuthFlow: Sized {
    type Request: Clone;
    type Response: Clone;
}

#[derive(Debug, Clone)]
pub struct FlowContext<T: AuthFlow> {
    pub request: T::Request,
    pub response: T::Response,
    _marker: PhantomData<T>,
}

impl<T: AuthFlow> FlowContext<T> {
    pub fn new(request: T::Request, response: T::Response) -> Self {
        Self { request, response, _marker: PhantomData }
    }
}

// API Key Flow

#[derive(Debug, Clone)]
pub struct ApiKeyAuthFlow;

#[derive(Debug, Clone)]
pub struct ApiKeyRequest {
    pub required_params: Vec<URLParam>,
}

#[derive(Debug, Clone)]
pub struct ApiKeyResponse {
    pub api_key: String,
    pub url_params: HashMap<String, String>,
}

impl AuthFlow for ApiKeyAuthFlow {
    type Request = ApiKeyRequest;
    type Response = ApiKeyResponse;
}

// Device Code Flow

#[derive(Debug, Clone)]
pub struct DeviceCodeAuthFlow;

#[derive(Debug, Clone)]
pub struct DeviceCodeRequest {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub interval: u64,
}

impl AuthFlow for DeviceCodeAuthFlow {
    type Request = DeviceCodeRequest;
    type Response = DeviceCodeResponse;
}

// Authorization Code Flow

#[derive(Debug, Clone)]
pub struct CodeAuthFlow;

#[derive(Debug, Clone)]
pub struct CodeRequest {
    pub authorization_url: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct CodeResponse {
    pub state: String,
    pub pkce_verifier: Option<String>,
}

impl AuthFlow for CodeAuthFlow {
    type Request = CodeRequest;
    type Response = CodeResponse;
}

// Runtime polymorphic flow context

#[derive(Debug, Clone)]
pub enum AuthContext {
    ApiKey(FlowContext<ApiKeyAuthFlow>),
    DeviceCode(FlowContext<DeviceCodeAuthFlow>),
    Code(FlowContext<CodeAuthFlow>),
}

impl AuthContext {
    pub fn api_key(request: ApiKeyRequest, response: ApiKeyResponse) -> Self {
        Self::ApiKey(FlowContext::new(request, response))
    }

    pub fn device_code(request: DeviceCodeRequest, response: DeviceCodeResponse) -> Self {
        Self::DeviceCode(FlowContext::new(request, response))
    }

    pub fn code(request: CodeRequest, response: CodeResponse) -> Self {
        Self::Code(FlowContext::new(request, response))
    }
}

// Completion types

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResponse {
    ApiKey {
        api_key: String,
        url_params: HashMap<String, String>,
    },
    Device {
        device_code: String,
        interval: u64,
    },
    Code {
        state: String,
        pkce_verifier: Option<String>,
    },
}

impl Default for AuthResponse {
    fn default() -> Self {
        Self::ApiKey { api_key: String::new(), url_params: HashMap::new() }
    }
}

impl AuthResponse {
    pub fn api_key(api_key: String, url_params: HashMap<String, String>) -> Self {
        Self::ApiKey { api_key, url_params }
    }

    pub fn device(device_code: String, interval: u64) -> Self {
        Self::Device { device_code, interval }
    }

    pub fn code(state: String, pkce_verifier: Option<String>) -> Self {
        Self::Code { state, pkce_verifier }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResult {
    ApiKey {
        api_key: String,
        url_params: HashMap<String, String>,
    },
    OAuthTokens {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
    },
    AuthorizationCode {
        code: String,
        state: String,
        code_verifier: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_more::Deref)]
#[serde(transparent)]
pub struct URLParam(String);

impl AsRef<str> for URLParam {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
