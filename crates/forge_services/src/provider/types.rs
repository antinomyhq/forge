/// Information to display to the user during OAuth device flow.
#[derive(Debug, Clone)]
pub struct OAuthDeviceDisplay {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

/// Result of validating provided credentials.
#[derive(Debug, Clone, Default, derive_setters::Setters)]
#[setters(strip_option, into)]
pub struct ValidationOutcome {
    pub success: bool,
    pub message: Option<String>,
}
