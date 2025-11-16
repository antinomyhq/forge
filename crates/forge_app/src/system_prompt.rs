use std::sync::Arc;

use derive_setters::Setters;
use forge_domain::inline_shell::{InlineShellCommand, parse_inline_commands};
use forge_domain::{
    Agent, Conversation, Environment, Error, Model, SystemContext, Template, ToolDefinition,
    ToolUsagePrompt,
};
use tracing::debug;

use crate::TemplateService;
use crate::inline_shell::{InlineShellExecutor, replace_commands_in_content};

#[derive(Setters)]
pub struct SystemPrompt<S> {
    services: Arc<S>,
    environment: Environment,
    agent: Agent,
    tool_definitions: Vec<ToolDefinition>,
    files: Vec<String>,
    models: Vec<Model>,
    custom_instructions: Vec<String>,
    inline_shell_executor: Arc<dyn InlineShellExecutor + Send + Sync>,
}

impl<S: TemplateService> SystemPrompt<S> {
    pub fn new(
        services: Arc<S>,
        environment: Environment,
        agent: Agent,
        inline_shell_executor: Arc<dyn InlineShellExecutor + Send + Sync>,
    ) -> Self {
        Self {
            services,
            environment,
            agent,
            models: Vec::default(),
            tool_definitions: Vec::default(),
            files: Vec::default(),
            custom_instructions: Vec::default(),
            inline_shell_executor,
        }
    }

    pub async fn add_system_message(
        &self,
        mut conversation: Conversation,
    ) -> anyhow::Result<Conversation> {
        let context = conversation.context.take().unwrap_or_default();
        let agent = &self.agent;
        let context = if let Some(system_prompt) = &agent.system_prompt {
            let env = self.environment.clone();
            let mut files = self.files.clone();
            files.sort();

            let tool_supported = self.is_tool_supported()?;
            let supports_parallel_tool_calls = self.is_parallel_tool_call_supported();
            let tool_information = match tool_supported {
                true => None,
                false => Some(ToolUsagePrompt::from(&self.tool_definitions).to_string()),
            };

            let mut custom_rules = Vec::new();

            agent.custom_rules.iter().for_each(|rule| {
                custom_rules.push(rule.as_str());
            });

            self.custom_instructions.iter().for_each(|rule| {
                custom_rules.push(rule.as_str());
            });

            let ctx = SystemContext {
                env: Some(env),
                tool_information,
                tool_supported,
                files,
                custom_rules: custom_rules.join("\n\n"),
                supports_parallel_tool_calls,
            };

            // Process inline shell commands in the system prompt template
            // Execute commands and replace with their actual results
            let parsed = parse_inline_commands(&system_prompt.template);
            let processed_template = if let Ok(parsed_content) = parsed {
                if parsed_content.commands_found.is_empty() {
                    system_prompt.template.clone()
                } else {
                    // Execute inline shell commands and replace with their results
                    let cwd = &self.environment.cwd;
                    let restricted = false; // Allow execution in system prompts
                    let commands: Vec<InlineShellCommand> = parsed_content.commands_found.to_vec();

                    match self
                        .inline_shell_executor
                        .execute_commands(commands, cwd, restricted)
                        .await
                    {
                        Ok(results) => {
                            replace_commands_in_content(&system_prompt.template, &results)
                        }
                        Err(e) => {
                            debug!(
                                "Failed to execute inline shell commands in system prompt: {}",
                                e
                            );
                            // Fallback to placeholder on error
                            let fallback_results = parsed_content
                                .commands_found
                                .iter()
                                .map(|cmd| forge_domain::CommandResult {
                                    original_match: cmd.full_match.clone(),
                                    command: cmd.command.clone(),
                                    stdout: format!("[Inline shell command failed: {}]", e),
                                    stderr: String::new(),
                                    exit_code: 1,
                                })
                                .collect::<Vec<_>>();
                            replace_commands_in_content(&system_prompt.template, &fallback_results)
                        }
                    }
                }
            } else {
                system_prompt.template.clone()
            };

            let static_block = self
                .services
                .render_template(Template::new(&processed_template), &())
                .await?;
            let non_static_block = self
                .services
                .render_template(Template::new("{{> forge-custom-agent-template.md }}"), &ctx)
                .await?;

            context.set_system_messages(vec![static_block, non_static_block])
        } else {
            context
        };

        Ok(conversation.context(context))
    }

    // Returns if agent supports tool or not.
    fn is_tool_supported(&self) -> anyhow::Result<bool> {
        let agent = &self.agent;
        let model_id = agent
            .model
            .as_ref()
            .ok_or(Error::MissingModel(agent.id.clone()))?;

        // Check if at agent level tool support is defined
        let tool_supported = match agent.tool_supported {
            Some(tool_supported) => tool_supported,
            None => {
                // If not defined at agent level, check model level

                let model = self.models.iter().find(|model| &model.id == model_id);
                model
                    .and_then(|model| model.tools_supported)
                    .unwrap_or_default()
            }
        };

        debug!(
            agent_id = %agent.id,
            model_id = %model_id,
            tool_supported,
            "Tool support check"
        );
        Ok(tool_supported)
    }

    /// Checks if parallel tool calls is supported by agent
    fn is_parallel_tool_call_supported(&self) -> bool {
        let agent = &self.agent;
        agent
            .model
            .as_ref()
            .and_then(|model_id| self.models.iter().find(|model| &model.id == model_id))
            .and_then(|model| model.supports_parallel_tool_calls)
            .unwrap_or_default()
    }
}
