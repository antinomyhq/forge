use std::collections::HashMap;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

// Security-Critical Credential Newtypes

/// API key for authentication
///
/// API key credential
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Hash, Debug,
)]
#[serde(transparent)]
pub struct ApiKey(String);

/// OAuth access token
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    PartialEq,
    Eq,
    Debug,
)]
#[serde(transparent)]
pub struct AccessToken(String);

/// OAuth refresh token
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct RefreshToken(String);

/// Device code for OAuth device flow
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct DeviceCode(String);

/// User code for OAuth device flow
///
/// This is displayed to the user, so it has a standard Debug implementation.
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct UserCode(String);

// OAuth Flow Newtypes

/// State parameter for CSRF protection in OAuth flows
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct State(String);

/// OAuth client identifier
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct ClientId(String);

/// PKCE code verifier
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct PkceVerifier(String);

/// Authorization code from OAuth code flow
#[derive(
    Clone, Serialize, Deserialize, derive_more::From, derive_more::Deref, PartialEq, Eq, Debug,
)]
#[serde(transparent)]
pub struct AuthorizationCode(String);

// URL Newtypes

/// OAuth authorization endpoint URL
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct AuthUrl(String);

/// OAuth token endpoint URL
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct TokenUrl(String);

/// Verification URI for device flow
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct VerificationUri(String);

/// Authorization URL for code flow
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct AuthorizationUrl(String);

// Identifier Newtypes

/// Session identifier
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    PartialEq,
    Eq,
    Debug,
)]
#[serde(transparent)]
pub struct SessionId(String);

/// API key name/label
#[derive(
    Clone,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Display,
    derive_more::Deref,
    Debug,
    PartialEq,
    Eq,
)]
#[serde(transparent)]
pub struct ApiKeyName(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub auth_url: AuthUrl,
    pub token_url: TokenUrl,
    pub client_id: ClientId,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub use_pkce: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_refresh_url: Option<TokenUrl>,
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
    type Method: Clone;
}

#[derive(Debug, Clone)]
pub struct FlowContext<T: AuthFlow> {
    pub request: T::Request,
    pub response: T::Response,
    pub method: T::Method,
    _marker: PhantomData<T>,
}

impl<T: AuthFlow> FlowContext<T> {
    pub fn new(request: T::Request, response: T::Response, method: T::Method) -> Self {
        Self { request, response, method, _marker: PhantomData }
    }
}

// API Key Flow

#[derive(Debug, Clone)]
pub struct ApiKeyAuthFlow;

#[derive(Debug, Clone)]
pub struct ApiKeyRequest {
    pub required_params: Vec<URLParam>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_more::Deref, derive_more::From,
)]
#[serde(transparent)]
pub struct URLParamValue(String);

#[derive(Debug, Clone)]
pub struct ApiKeyResponse {
    pub api_key: ApiKey,
    pub url_params: HashMap<URLParam, URLParamValue>,
}

#[derive(Debug, Clone)]
pub struct ApiKeyMethod;

impl AuthFlow for ApiKeyAuthFlow {
    type Request = ApiKeyRequest;
    type Response = ApiKeyResponse;
    type Method = ApiKeyMethod;
}

// Device Code Flow

#[derive(Debug, Clone)]
pub struct DeviceCodeAuthFlow;

#[derive(Debug, Clone)]
pub struct DeviceCodeRequest {
    pub user_code: UserCode,
    pub verification_uri: VerificationUri,
    pub verification_uri_complete: Option<VerificationUri>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: DeviceCode,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub struct DeviceCodeMethod {
    pub oauth_config: OAuthConfig,
}

impl AuthFlow for DeviceCodeAuthFlow {
    type Request = DeviceCodeRequest;
    type Response = DeviceCodeResponse;
    type Method = DeviceCodeMethod;
}

// Authorization Code Flow

#[derive(Debug, Clone)]
pub struct CodeAuthFlow;

#[derive(Debug, Clone)]
pub struct CodeRequest {
    pub authorization_url: AuthorizationUrl,
    pub state: State,
}

#[derive(Debug, Clone)]
pub struct CodeResponse {
    pub state: State,
    pub pkce_verifier: Option<PkceVerifier>,
}

#[derive(Debug, Clone)]
pub struct CodeMethod {
    pub oauth_config: OAuthConfig,
}

impl AuthFlow for CodeAuthFlow {
    type Request = CodeRequest;
    type Response = CodeResponse;
    type Method = CodeMethod;
}

// Runtime polymorphic flow context

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

// Completion types

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

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    Hash,
    derive_more::From,
    derive_more::Display,
)]
#[serde(transparent)]
pub struct URLParam(String);
