use super::{AuthFlow, DeviceCode, OAuthConfig, UserCode, VerificationUri};

/// Device code OAuth authentication flow
#[derive(Debug, Clone)]
pub struct DeviceCodeAuthFlow;

/// Request parameters for device code flow
#[derive(Debug, Clone)]
pub struct DeviceCodeRequest {
    pub user_code: UserCode,
    pub verification_uri: VerificationUri,
    pub verification_uri_complete: Option<VerificationUri>,
    pub expires_in: u64,
    pub interval: u64,
}

/// Response containing device code and polling interval
#[derive(Debug, Clone)]
pub struct DeviceCodeResponse {
    pub device_code: DeviceCode,
    pub interval: u64,
}

/// Method configuration for device code flow
#[derive(Debug, Clone)]
pub struct DeviceCodeMethod {
    pub oauth_config: OAuthConfig,
}

impl AuthFlow for DeviceCodeAuthFlow {
    type Request = DeviceCodeRequest;
    type Response = DeviceCodeResponse;
    type Method = DeviceCodeMethod;
}
