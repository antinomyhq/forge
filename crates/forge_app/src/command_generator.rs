use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use forge_domain::*;

use crate::{AppConfigService, EnvironmentService, ProviderService, Services, TemplateService};

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

        // Read command history if available
        let history_context = self.get_command_history().await.unwrap_or_default();

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

        Ok(message.content)
    }

    /// Get recent command history for context
    async fn get_command_history(&self) -> Result<Vec<String>> {
        let env = self.services.environment_service().get_environment();

        // First try to use HISTFILE environment variable
        let history_file = std::env::var("HISTFILE")
            .ok()
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .or_else(|| {
                let home = env.home.as_ref()?;

                match env.shell.as_str() {
                    s if s.contains("zsh") => {
                        let path = home.join(".zsh_history");
                        path.exists().then_some(path)
                    }
                    s if s.contains("bash") => {
                        let path = home.join(".bash_history");
                        path.exists().then_some(path)
                    }
                    _ => None,
                }
            });

        if let Some(history_path) = history_file {
            // Read the history file directly, handling potential non-UTF-8 bytes
            if let Ok(bytes) = tokio::fs::read(&history_path).await {
                let content = String::from_utf8_lossy(&bytes);
                let all_commands: Vec<String> = content
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(|s| s.to_string())
                    .collect();

                // Take the last 10 commands in chronological order
                let start = all_commands.len().saturating_sub(10);
                let commands = all_commands[start..].to_vec();

                return Ok(commands);
            }
        }

        Ok(Vec::new())
    }
}
