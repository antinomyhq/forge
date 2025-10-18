use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_app::dto::{
    InitAuth, LoginInfo, OAuthTokens, Provider, ProviderCredential, ProviderId, ToolsOverview,
};
use forge_app::{
    AgentLoaderService, AuthService, ConversationService, EnvironmentService, FileDiscoveryService,
    ForgeApp, McpConfigManager, McpService, ProviderRegistry, ProviderService, Services, User,
    UserUsage, Walker, WorkflowService,
};
use forge_domain::*;
use forge_infra::ForgeInfra;
use forge_services::provider::{
    ImportSummary, OAuthDeviceDisplay, ProviderMetadataService, ValidationOutcome, ValidationResult,
};
use forge_services::{
    AppConfigRepository, CommandInfra, EnvironmentInfra, ForgeServices, HttpInfra, OAuthFlowInfra,
    ProviderCredentialRepository, ProviderSpecificProcessingInfra, ProviderValidationInfra,
};
use forge_stream::MpscStream;

use crate::API;

pub struct ForgeAPI<S, F> {
    services: Arc<S>,
    infra: Arc<F>,
}

impl<A, F> ForgeAPI<A, F> {
    pub fn new(services: Arc<A>, infra: Arc<F>) -> Self {
        Self { services, infra }
    }
}

impl ForgeAPI<ForgeServices<ForgeInfra>, ForgeInfra> {
    pub fn init(restricted: bool, cwd: PathBuf) -> Self {
        let infra = Arc::new(ForgeInfra::new(restricted, cwd));
        let app = Arc::new(ForgeServices::new(infra.clone()));
        ForgeAPI::new(app, infra)
    }
}

#[async_trait::async_trait]
impl<
    A: Services,
    F: CommandInfra
        + AppConfigRepository
        + ProviderCredentialRepository
        + HttpInfra
        + EnvironmentInfra
        + ProviderValidationInfra
        + OAuthFlowInfra
        + ProviderSpecificProcessingInfra,
> API for ForgeAPI<A, F>
{
    async fn discover(&self) -> Result<Vec<File>> {
        let environment = self.services.get_environment();
        let config = Walker::unlimited().cwd(environment.cwd);
        self.services.collect_files(config).await
    }

    async fn tools(&self) -> anyhow::Result<ToolsOverview> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.list_tools().await
    }

    async fn models(&self) -> Result<Vec<Model>> {
        Ok(self
            .services
            .models(
                self.get_provider()
                    .await
                    .context("Failed to fetch models")?,
            )
            .await?)
    }
    async fn get_agents(&self) -> Result<Vec<Agent>> {
        Ok(self.services.get_agents().await?)
    }

    async fn providers(&self) -> Result<Vec<Provider>> {
        Ok(self.services.get_all_providers().await?)
    }

    async fn chat(
        &self,
        chat: ChatRequest,
    ) -> anyhow::Result<MpscStream<Result<ChatResponse, anyhow::Error>>> {
        // Create a ForgeApp instance and delegate the chat logic to it
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.chat(chat).await
    }

    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        self.services.upsert_conversation(conversation).await
    }

    async fn compact_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<CompactionResult> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.compact_conversation(conversation_id).await
    }

    fn environment(&self) -> Environment {
        self.services.get_environment().clone()
    }

    async fn read_workflow(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let app = ForgeApp::new(self.services.clone());
        app.read_workflow(path).await
    }

    async fn read_merged(&self, path: Option<&Path>) -> anyhow::Result<Workflow> {
        let app = ForgeApp::new(self.services.clone());
        app.read_workflow_merged(path).await
    }

    async fn write_workflow(&self, path: Option<&Path>, workflow: &Workflow) -> anyhow::Result<()> {
        let app = ForgeApp::new(self.services.clone());
        app.write_workflow(path, workflow).await
    }

    async fn update_workflow<T>(&self, path: Option<&Path>, f: T) -> anyhow::Result<Workflow>
    where
        T: FnOnce(&mut Workflow) + Send,
    {
        self.services.update_workflow(path, f).await
    }

    async fn conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        self.services.find_conversation(conversation_id).await
    }

    async fn list_conversations(&self, limit: Option<usize>) -> anyhow::Result<Vec<Conversation>> {
        Ok(self
            .services
            .get_conversations(limit)
            .await?
            .unwrap_or_default())
    }

    async fn last_conversation(&self) -> anyhow::Result<Option<Conversation>> {
        self.services.last_conversation().await
    }

    async fn execute_shell_command(
        &self,
        command: &str,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.infra
            .execute_command(command.to_string(), working_dir, false, None)
            .await
    }
    async fn read_mcp_config(&self, scope: Option<&Scope>) -> Result<McpConfig> {
        self.services
            .read_mcp_config(scope)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn write_mcp_config(&self, scope: &Scope, config: &McpConfig) -> Result<()> {
        self.services
            .write_mcp_config(config, scope)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    async fn execute_shell_command_raw(
        &self,
        command: &str,
    ) -> anyhow::Result<std::process::ExitStatus> {
        let cwd = self.environment().cwd;
        self.infra.execute_command_raw(command, cwd, None).await
    }

    async fn init_login(&self) -> Result<InitAuth> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.init_auth().await
    }

    async fn login(&self, auth: &InitAuth) -> Result<()> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.login(auth).await
    }

    async fn logout(&self) -> Result<()> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.logout().await
    }
    async fn get_provider(&self) -> anyhow::Result<Provider> {
        self.services.get_active_provider().await
    }

    async fn set_provider(&self, provider_id: ProviderId) -> anyhow::Result<()> {
        self.services.set_active_provider(provider_id).await
    }

    async fn user_info(&self) -> Result<Option<User>> {
        let provider = self.get_provider().await?;
        if let Some(ref api_key) = provider.key {
            let user_info = self.services.user_info(api_key).await?;
            return Ok(Some(user_info));
        }
        Ok(None)
    }

    async fn user_usage(&self) -> Result<Option<UserUsage>> {
        let provider = self.get_provider().await?;
        if let Some(ref api_key) = provider.key {
            let user_usage = self.services.user_usage(api_key).await?;
            return Ok(Some(user_usage));
        }
        Ok(None)
    }

    async fn get_operating_agent(&self) -> Option<AgentId> {
        self.services.get_active_agent().await.ok().flatten()
    }

    async fn set_operating_agent(&self, agent_id: AgentId) -> anyhow::Result<()> {
        self.services.set_active_agent(agent_id).await
    }

    async fn get_operating_model(&self) -> Option<ModelId> {
        self.services.get_active_model().await.ok()
    }

    async fn set_operating_model(&self, model_id: ModelId) -> anyhow::Result<()> {
        self.services.set_active_model(model_id).await
    }

    async fn get_login_info(&self) -> Result<Option<LoginInfo>> {
        self.services.auth_service().get_auth_token().await
    }

    async fn reload_mcp(&self) -> Result<()> {
        self.services.mcp_service().reload_mcp().await
    }

    async fn available_provider_ids(&self) -> Result<Vec<ProviderId>> {
        Ok(self.services.available_provider_ids())
    }

    async fn list_provider_credentials(&self) -> Result<Vec<ProviderCredential>> {
        self.infra.get_all_credentials().await
    }

    async fn get_provider_credential(
        &self,
        provider_id: &ProviderId,
    ) -> Result<Option<ProviderCredential>> {
        self.infra.get_credential(provider_id).await
    }

    async fn upsert_provider_credential(&self, credential: ProviderCredential) -> Result<()> {
        self.infra.upsert_credential(credential).await
    }

    async fn delete_provider_credential(&self, provider_id: &ProviderId) -> Result<()> {
        self.infra.delete_credential(provider_id).await
    }

    async fn validate_provider_credential(&self, credential: &ProviderCredential) -> Result<bool> {
        use forge_services::provider::validation::{
            ForgeProviderValidationService, ValidationResult,
        };

        // Get the provider to access validation URL
        let providers = self.providers().await?;
        let provider = providers
            .iter()
            .find(|p| p.id == credential.provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", credential.provider_id))?;

        // Create validation service and validate
        let validation_service = ForgeProviderValidationService::new(self.infra.clone());
        let result = validation_service
            .validate_credential(&credential.provider_id, credential, &provider.model_url)
            .await?;

        match result {
            ValidationResult::Valid => Ok(true),
            ValidationResult::Invalid(msg) => Err(anyhow::anyhow!("Invalid credentials: {}", msg)),
            ValidationResult::TokenExpired => Err(anyhow::anyhow!("OAuth token has expired")),
            ValidationResult::Inconclusive(msg) => {
                // For inconclusive results, we'll treat as an error but with a different
                // message
                Err(anyhow::anyhow!("Could not validate credentials: {}", msg))
            }
        }
    }

    async fn mark_credential_verified(&self, provider_id: &ProviderId) -> Result<()> {
        self.infra.mark_verified(provider_id).await
    }

    // High-level provider authentication methods
    async fn add_provider_api_key(
        &self,
        provider_id: ProviderId,
        api_key: String,
        skip_validation: bool,
    ) -> Result<ValidationOutcome> {
        if !skip_validation {
            self.infra.validate_api_key_format(&provider_id, &api_key)?;
        }

        let credential = ProviderCredential::new_api_key(provider_id, api_key);

        if !skip_validation {
            let providers = self.services.get_all_providers().await?;
            let provider = providers
                .iter()
                .find(|p| p.id == credential.provider_id)
                .ok_or_else(|| anyhow::anyhow!("Provider not found: {}", credential.provider_id))?;

            let result = self
                .infra
                .validate_credential(&credential.provider_id, &credential, &provider.model_url)
                .await?;

            match result {
                ValidationResult::Valid => {
                    self.infra.upsert_credential(credential).await?;
                    Ok(ValidationOutcome::success_with_message(
                        "API key validated and saved",
                    ))
                }
                ValidationResult::Invalid(msg) => Ok(ValidationOutcome::failure(format!(
                    "API key validation failed: {}",
                    msg
                ))),
                ValidationResult::Inconclusive(msg) => {
                    self.infra.upsert_credential(credential).await?;
                    Ok(ValidationOutcome::success_with_message(format!(
                        "API key saved (validation inconclusive: {})",
                        msg
                    )))
                }
                ValidationResult::TokenExpired => {
                    Ok(ValidationOutcome::failure("Token has expired"))
                }
            }
        } else {
            self.infra.upsert_credential(credential).await?;
            Ok(ValidationOutcome::success_with_message(
                "API key saved without validation",
            ))
        }
    }

    async fn authenticate_provider_oauth<Cb>(
        &self,
        provider_id: ProviderId,
        display_callback: Cb,
    ) -> Result<()>
    where
        Cb: FnOnce(OAuthDeviceDisplay) + Send,
    {
        let metadata = self.infra.get_provider_metadata(&provider_id);
        let oauth_config = metadata
            .auth_methods
            .iter()
            .find_map(|method| method.oauth_config.clone())
            .ok_or_else(|| anyhow::anyhow!("Provider {} does not support OAuth", provider_id))?;

        let oauth_tokens = self
            .infra
            .device_flow_with_callback(&oauth_config, display_callback)
            .await?;

        let credential = match provider_id {
            ProviderId::GithubCopilot => {
                let (api_key, expires_at) = self
                    .infra
                    .process_github_copilot_token(&oauth_tokens.access_token)
                    .await?;
                let expires_at = expires_at.unwrap_or(oauth_tokens.expires_at);

                let copilot_tokens = OAuthTokens {
                    access_token: oauth_tokens.access_token.clone(),
                    refresh_token: oauth_tokens.refresh_token.clone(),
                    expires_at,
                };

                ProviderCredential::new_oauth_with_api_key(provider_id, api_key, copilot_tokens)
            }
            _ => ProviderCredential::new_oauth(provider_id, oauth_tokens),
        };

        self.infra.upsert_credential(credential).await?;
        Ok(())
    }

    async fn import_provider_credentials_from_env(
        &self,
        filter: Option<ProviderId>,
    ) -> Result<ImportSummary> {
        let mut summary = ImportSummary::new();

        for provider_id in ProviderMetadataService::provider_ids() {
            if let Some(filter_id) = filter
                && filter_id != provider_id
            {
                continue;
            }

            let env_var_names = self.infra.get_provider_metadata(&provider_id).env_var_names;
            let api_key = env_var_names
                .iter()
                .find_map(|var_name| self.infra.get_env_var(var_name))
                .filter(|key| !key.is_empty());

            match api_key {
                Some(key) => {
                    let credential = ProviderCredential::new_api_key(provider_id, key);
                    match self.infra.upsert_credential(credential).await {
                        Ok(_) => summary.imported.push(provider_id),
                        Err(e) => summary.failed.push((provider_id, e.to_string())),
                    }
                }
                None => summary.skipped.push(provider_id),
            }
        }

        Ok(summary)
    }
}
