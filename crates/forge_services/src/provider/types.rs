
/// Information to display to the user during OAuth device flow.
#[derive(Debug, Clone)]
pub struct OAuthDeviceDisplay {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

/// Result of validating provided credentials.
#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    pub success: bool,
    pub message: Option<String>,
}

impl ValidationOutcome {
    pub fn success() -> Self {
        Self { success: true, message: None }
    }

    pub fn success_with_message(message: impl Into<String>) -> Self {
        Self { success: true, message: Some(message.into()) }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self { success: false, message: Some(message.into()) }
    }
}
