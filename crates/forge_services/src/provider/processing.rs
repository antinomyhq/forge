use chrono::{DateTime, Utc};
use forge_app::dto::ProviderId;

use crate::infra::ProviderSpecificProcessingInfra;
use crate::provider::GitHubCopilotService;
use crate::provider::metadata::{ProviderMetadata, ProviderMetadataService};

/// Infrastructure adapter providing provider-specific processing capabilities.
///
/// This type bridges provider metadata lookups and provider-specific OAuth
/// post-processing required by orchestrators in higher layers.
#[derive(Default)]
pub struct ProviderProcessingService;

impl ProviderProcessingService {
    /// Creates a new processing service.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ProviderSpecificProcessingInfra for ProviderProcessingService {
    async fn process_github_copilot_token(
        &self,
        access_token: &str,
    ) -> anyhow::Result<(String, Option<DateTime<Utc>>)> {
        let service = GitHubCopilotService::new();
        let (api_key, expires_at) = service.get_copilot_api_key(access_token).await?;
        Ok((api_key, Some(expires_at)))
    }

    fn get_provider_metadata(&self, provider_id: &ProviderId) -> ProviderMetadata {
        ProviderMetadataService::get_metadata(provider_id)
    }
}
