use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::GrpcInfra;
use forge_domain::{ApiKey, ApiKeyInfo, AuthFlowLoginInfo, AuthFlowRepository, InitFlowResponse};

use crate::proto_generated::forge_service_client::ForgeServiceClient;
use crate::proto_generated::{
    DeleteApiKeyRequest, GetApiKeysRequest, InitFlowRequest, PollRequest,
};

/// gRPC implementation of AuthFlowRepository
///
/// This repository provides authentication flow operations via gRPC.
pub struct ForgeAuthFlowRepository<I> {
    infra: Arc<I>,
}

impl<I> ForgeAuthFlowRepository<I> {
    /// Create a new repository with the given infrastructure
    ///
    /// # Arguments
    /// * `infra` - Infrastructure that provides gRPC connection
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}

#[async_trait]
impl<I: GrpcInfra> AuthFlowRepository for ForgeAuthFlowRepository<I> {
    async fn init_flow(&self) -> Result<InitFlowResponse> {
        let request = tonic::Request::new(InitFlowRequest {});

        let channel = self.infra.channel();
        let mut client = ForgeServiceClient::new(channel);
        let response = client
            .init_flow(request)
            .await
            .context("Failed to call InitFlow gRPC")?
            .into_inner();

        response.try_into()
    }

    async fn poll_auth(
        &self,
        session_id: &str,
        iv: &str,
        aad: &str,
    ) -> Result<Option<AuthFlowLoginInfo>> {
        let request = tonic::Request::new(PollRequest {
            session_id: session_id.to_string(),
            iv: iv.to_string(),
            aad: aad.to_string(),
        });

        let channel = self.infra.channel();
        let mut client = ForgeServiceClient::new(channel);
        let response = client
            .poll(request)
            .await
            .context("Failed to call Poll gRPC")?
            .into_inner();

        // Convert proto response to domain type
        // Returns None if login_info is not present (auth still pending)
        match response.login_info {
            Some(login_info) => {
                let domain_login = login_info.try_into()?;
                Ok(Some(domain_login))
            }
            None => Ok(None),
        }
    }

    async fn get_api_keys(&self, token: &ApiKey) -> Result<Vec<ApiKeyInfo>> {
        let mut request = tonic::Request::new(GetApiKeysRequest {});

        // Add authorization header
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {}", &**token).parse()?);

        let channel = self.infra.channel();
        let mut client = ForgeServiceClient::new(channel);
        let response = client
            .get_api_keys(request)
            .await
            .context("Failed to call GetApiKeys gRPC")?
            .into_inner();

        // Convert proto API keys to domain types
        response
            .api_keys
            .into_iter()
            .map(|key| key.try_into())
            .collect()
    }

    async fn delete_api_key(&self, token: &ApiKey, key_id: &str) -> Result<()> {
        let mut request = tonic::Request::new(DeleteApiKeyRequest { id: key_id.to_string() });

        // Add authorization header
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {}", &**token).parse()?);

        let channel = self.infra.channel();
        let mut client = ForgeServiceClient::new(channel);
        client
            .delete_api_key(request)
            .await
            .context("Failed to call DeleteApiKey gRPC")?;

        Ok(())
    }
}
