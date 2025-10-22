/// OAuth device flow display info
#[derive(Debug, Clone)]
pub struct OAuthDeviceDisplay {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}
