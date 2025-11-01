use std::sync::Arc;

use bytes::Bytes;
use forge_app::{EnvironmentInfra, FileReaderInfra, FileWriterInfra};
use forge_domain::{AuthCredential, AuthRepository, ProviderId};

/// Repository for managing authentication credentials.
///
/// This repository stores credentials in a JSON file at
/// `env.base_path.join(".provider_credentials.json")`. Credentials are indexed
/// by ProviderId for efficient lookup.
#[derive(Debug)]
pub struct AuthCredentialRepo<F> {
    infra: Arc<F>,
}

impl<F> AuthCredentialRepo<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

impl<F: EnvironmentInfra + FileWriterInfra + FileReaderInfra> AuthCredentialRepo<F> {
    async fn read_credentials(&self) -> Vec<AuthCredential> {
        let path = self
            .infra
            .get_environment()
            .base_path
            .join(".provider_credentials.json");

        match self.infra.read_utf8(&path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
    async fn write_credentials(&self, credentials: &Vec<AuthCredential>) -> anyhow::Result<()> {
        let path = self
            .infra
            .get_environment()
            .base_path
            .join(".provider_credentials.json");

        let content = serde_json::to_string_pretty(credentials)?;
        self.infra.write(&path, Bytes::from(content)).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra + FileReaderInfra + FileWriterInfra + Send + Sync> AuthRepository
    for AuthCredentialRepo<F>
{
    async fn upsert_credential(&self, credential: AuthCredential) -> anyhow::Result<()> {
        let mut credentials = self.read_credentials().await;
        let id = credential.id;
        // Update existing credential or add new one
        if let Some(existing) = credentials.iter_mut().find(|c| c.id == id) {
            *existing = credential;
        } else {
            credentials.push(credential);
        }
        self.write_credentials(&credentials).await?;

        Ok(())
    }

    async fn get_credential(&self, id: &ProviderId) -> anyhow::Result<Option<AuthCredential>> {
        let credentials = self.read_credentials().await;
        Ok(credentials.into_iter().find(|c| &c.id == id))
    }
}
