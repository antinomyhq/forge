use super::{DeviceCode, OAuthConfig, UserCode, VerificationUri};

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
