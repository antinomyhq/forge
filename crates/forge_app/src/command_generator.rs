use std::sync::Arc;

use anyhow::Result;
use forge_domain::{extract_tag_content, *};

use crate::{
    AppConfigService, EnvironmentInfra, EnvironmentService, ExecutedCommands, FileReaderInfra,
    ProviderService, Services, TemplateService,
};

/// CommandGenerator handles shell command generation from natural language
pub struct CommandGenerator<S, F> {
    services: Arc<S>,
    infra: Arc<F>,
}

impl<S: Services, F: EnvironmentInfra + FileReaderInfra> CommandGenerator<S, F> {
    /// Creates a new CommandGenerator instance with the provided services.
    pub fn new(services: Arc<S>, infra: Arc<F>) -> Self {
        Self { services, infra }
    }

    /// Generates a shell command from a natural language prompt
    pub async fn generate(&self, prompt: UserPrompt) -> Result<String> {
        // Get system information for context
        let env = self.services.environment_service().get_environment();

        // Read command history if available
        let limit: usize = self
            .infra
            .get_env_var("FORGE_COMMAND_HISTORY_LIMIT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);
        let history_context = ExecutedCommands::new(self.infra.clone())
            .shell_commands(&env, limit)
            .await?;

        let rendered_system_prompt = self
            .services
            .render_template(
                Template::new("{{> forge-command-generator-prompt.md }}"),
                &serde_json::json!({
                    "env": env,
                }),
            )
            .await?;

        // Get required services and data
        let provider = self.services.get_default_provider().await?;
        let model = self.services.get_default_model(&provider.id).await?;

        // Build user prompt with task and recent commands
        let user_content = if history_context.is_empty() {
            format!("<task>{}</task>", prompt.as_str())
        } else {
            format!(
                "<recently_executed_commands>\n{}\n</recently_executed_commands>\n\n<task>{}</task>",
                history_context
                    .into_iter()
                    .map(|h| format!("- {}", h))
                    .collect::<Vec<_>>()
                    .join("\n"),
                prompt.as_str()
            )
        };

        // Create context with system and user prompts
        let ctx = Context::default()
            .add_message(ContextMessage::system(rendered_system_prompt))
            .add_message(ContextMessage::user(user_content, Some(model.clone())));

        // Send message to LLM
        let stream = self
            .services
            .provider_service()
            .chat(&model, ctx, provider)
            .await?;
        let message = stream.into_full(false).await?;

        // Extract the command from the <command> tag
        let command = extract_tag_content(&message.content, "command")
            .ok_or_else(|| anyhow::anyhow!("Failed to extract <command> tag from LLM response"))?;

        Ok(command.to_string())
    }
}
