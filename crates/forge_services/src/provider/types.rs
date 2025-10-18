use forge_app::dto::ProviderId;

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

/// Summary of results when importing credentials from the environment.
#[derive(Debug, Clone, Default)]
pub struct ImportSummary {
    pub imported: Vec<ProviderId>,
    pub failed: Vec<(ProviderId, String)>,
    pub skipped: Vec<ProviderId>,
}

impl ImportSummary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total_processed(&self) -> usize {
        self.imported.len() + self.failed.len() + self.skipped.len()
    }

    pub fn has_imports(&self) -> bool {
        !self.imported.is_empty()
    }

    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }
}
