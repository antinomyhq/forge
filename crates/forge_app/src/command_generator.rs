use std::sync::Arc;

use anyhow::Result;
use forge_domain::{extract_tag_content, *};

use crate::{AppConfigService, EnvironmentService, ProviderService, Services, TemplateEngine};

/// CommandGenerator handles shell command generation from natural language
pub struct CommandGenerator<S> {
    services: Arc<S>,
}

impl<S: Services> CommandGenerator<S> {
    /// Creates a new CommandGenerator instance with the provided services.
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Generates a shell command from a natural language prompt
    pub async fn generate(&self, prompt: UserPrompt) -> Result<String> {
        // Get system information for context
        let env = self.services.environment_service().get_environment();

        let rendered_system_prompt = TemplateEngine::default().render(
            "forge-command-generator-prompt.md",
            &serde_json::json!({"env": env}),
        )?;

        // Get required services and data
        let provider = self.services.get_default_provider().await?;
        let model = self.services.get_default_model(&provider.id).await?;

        // Build user prompt with task and recent commands
        let user_content = format!("<task>{}</task>", prompt.as_str());

        // Create context with system and user prompts
        let ctx = self.create_context(rendered_system_prompt, user_content, &model);

        // Send message to LLM
        let stream = self
            .services
            .provider_service()
            .chat(&model, ctx, provider)
            .await?;
        let message = stream.into_full(false).await?;

        // Extract the command from the <shell_command> tag
        let command = extract_tag_content(&message.content, "shell_command").ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to generate shell command: Unexpected response: {}",
                message.content
            )
        })?;

        Ok(command.to_string())
    }

    /// Creates a context with system and user messages for the LLM
    fn create_context(
        &self,
        system_prompt: String,
        user_content: String,
        model: &ModelId,
    ) -> Context {
        Context::default()
            .add_message(ContextMessage::system(system_prompt))
            .add_message(ContextMessage::user(user_content, Some(model.clone())))
    }
}
