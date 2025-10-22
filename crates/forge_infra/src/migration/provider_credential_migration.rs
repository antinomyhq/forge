use std::sync::Arc;

use anyhow::Result;
use forge_app::dto::{ProviderCredential, ProviderId};
use forge_services::provider::registry::get_provider_credential_vars;
use forge_services::{EnvironmentInfra, ProviderCredentialRepository};
use strum::IntoEnumIterator;

pub struct ProviderCredentialMigration<E, R> {
    env_infra: Arc<E>,
    credential_repo: Arc<R>,
}

impl<E, R> ProviderCredentialMigration<E, R> {
    pub fn new(env_infra: Arc<E>, credential_repo: Arc<R>) -> Self {
        Self { env_infra, credential_repo }
    }
}

impl<E: EnvironmentInfra, R: ProviderCredentialRepository> ProviderCredentialMigration<E, R> {
    pub async fn run(&self) -> Result<()> {
        if !self.credential_repo.get_all_credentials().await?.is_empty() {
            return Ok(());
        }

        let mut imported = 0;
        for provider_id in ProviderId::iter() {
            if self.migrate_provider(&provider_id).await?.is_some() {
                imported += 1;
            }
        }

        if imported > 0 {
            tracing::info!(imported, "Migrated provider credentials from environment");
        }

        Ok(())
    }

    async fn migrate_provider(
        &self,
        provider_id: &ProviderId,
    ) -> Result<Option<ProviderCredential>> {
        let Some((api_key_var, url_param_vars)) = get_provider_credential_vars(provider_id) else {
            return Ok(None);
        };

        let Some(api_key) = self
            .env_infra
            .get_env_var(&api_key_var)
            .filter(|k| !k.trim().is_empty())
        else {
            return Ok(None);
        };

        let credential = ProviderCredential::new_api_key(provider_id.clone(), api_key).url_params(
            url_param_vars
                .iter()
                .filter_map(|var| {
                    self.env_infra
                        .get_env_var(var.as_str())
                        .filter(|v| !v.trim().is_empty())
                        .map(|val| (var.as_ref().to_string(), val))
                })
                .collect::<std::collections::HashMap<_, _>>(),
        );
        self.credential_repo
            .upsert_credential(credential.clone())
            .await?;

        Ok(Some(credential))
    }
}
