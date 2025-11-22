use std::sync::Arc;

use anyhow::Result;
use forge_domain::CommandResult;
use forge_domain::inline_shell::{InlineShellError, parse_inline_commands};
use tracing::{debug, warn};

use super::{SecurityContext, SecurityValidationService};
use crate::inline_shell::InlineShellExecutor;

/// Trait for processing inline commands in prompts
#[async_trait::async_trait]
pub trait PromptProcessor: Send + Sync {
    /// Process inline commands in the given content
    async fn process_inline_commands(
        &self,
        content: &str,
        security_context: &SecurityContext,
    ) -> Result<String>;
}

/// Default implementation of prompt processor
pub struct ForgePromptProcessor {
    security_service: SecurityValidationService,
    inline_shell_executor: Arc<dyn InlineShellExecutor + Send + Sync>,
}

impl ForgePromptProcessor {
    /// Create a new prompt processor
    pub fn new(
        security_service: SecurityValidationService,
        inline_shell_executor: Arc<dyn InlineShellExecutor + Send + Sync>,
    ) -> Self {
        Self { security_service, inline_shell_executor }
    }
}

#[async_trait::async_trait]
impl PromptProcessor for ForgePromptProcessor {
    async fn process_inline_commands(
        &self,
        content: &str,
        security_context: &SecurityContext,
    ) -> Result<String> {
        debug!(
            "Processing inline commands for context: {:?}",
            security_context.prompt_context
        );

        // Parse inline commands from content
        let parsed = parse_inline_commands(content)?;

        if parsed.commands_found.is_empty() {
            debug!("No inline commands found in content");
            return Ok(content.to_string());
        }

        debug!(
            "Found {} inline commands to execute",
            parsed.commands_found.len()
        );

        // Validate commands for security - only if restricted mode is enabled
        if security_context.restricted {
            for command in &parsed.commands_found {
                let command_str = &command.command;

                // Check if command is blocked in restricted mode
                if self
                    .security_service
                    .is_command_blocked(command_str, security_context.restricted)?
                {
                    warn!(
                        "Command blocked in restricted mode: {} (Context: {:?})",
                        command_str, security_context.prompt_context
                    );
                    return Err(InlineShellError::RestrictedModeBlocked {
                        command: command_str.to_string(),
                    }
                    .into());
                }

                // Check if command is in allowed list (if specified)
                if let Some(allowed_commands) = &security_context.allowed_commands {
                    let base_command = command_str.split_whitespace().next().unwrap_or(command_str);
                    if !allowed_commands.contains(&base_command.to_string()) {
                        warn!(
                            "Command not in allowed list: {} (Context: {:?})",
                            command_str, security_context.prompt_context
                        );
                        return Err(InlineShellError::PolicyBlocked {
                            command: command_str.to_string(),
                        }
                        .into());
                    }
                }
            }
        }

        // Execute commands
        let results = self
            .inline_shell_executor
            .execute_commands(
                parsed.commands_found.clone(),
                &security_context.cwd,
                security_context.restricted,
            )
            .await?;

        // Replace commands with their results
        let processed_content = replace_commands_in_content(content, &results);

        debug!("Successfully processed {} inline commands", results.len());

        Ok(processed_content)
    }
}

/// Replace inline commands in content with their execution results
fn replace_commands_in_content(content: &str, results: &[CommandResult]) -> String {
    let mut processed_content = content.to_string();

    for result in results {
        processed_content = processed_content.replace(&result.original_match, &result.stdout);
    }

    processed_content
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::MockInlineShellExecutor;

    #[tokio::test]
    async fn test_process_inline_commands_no_commands() {
        let executor = Arc::new(MockInlineShellExecutor::new());
        let processor = ForgePromptProcessor::new(SecurityValidationService::new(), executor);

        let content = "Hello world, no commands here";
        let context = SecurityContext::system(PathBuf::from("/test"));

        let result = processor
            .process_inline_commands(content, &context)
            .await
            .unwrap();
        assert_eq!(result, content);
    }

    #[tokio::test]
    async fn test_process_inline_commands_with_allowed_command() {
        let executor = Arc::new(MockInlineShellExecutor::new());
        executor.add_command_result("ls".to_string(), "file1.txt\nfile2.txt".to_string());

        let processor = ForgePromptProcessor::new(SecurityValidationService::new(), executor);

        let content = "List files: ![ls]";
        let context = SecurityContext::system(PathBuf::from("/test"));

        let result = processor
            .process_inline_commands(content, &context)
            .await
            .unwrap();
        assert!(result.contains("file1.txt"));
        assert!(result.contains("file2.txt"));
        assert!(!result.contains("![ls]"));
    }

    #[tokio::test]
    async fn test_process_inline_commands_blocked_command() {
        let executor = Arc::new(MockInlineShellExecutor::new());
        let processor = ForgePromptProcessor::new(SecurityValidationService::new(), executor);

        let content = "Dangerous command: ![rm -rf /]";
        let context = SecurityContext::system(PathBuf::from("/test"));

        let result = processor.process_inline_commands(content, &context).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("blocked in restricted mode")
        );
    }

    #[tokio::test]
    async fn test_process_inline_commands_not_in_allowed_list() {
        let executor = Arc::new(MockInlineShellExecutor::new());
        let processor = ForgePromptProcessor::new(SecurityValidationService::new(), executor);

        let content = "Try custom command: ![custom-cmd]";
        let context = SecurityContext::custom_command(
            PathBuf::from("/test"),
            vec!["allowed-cmd".to_string()],
        );

        let result = processor.process_inline_commands(content, &context).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("blocked by policy")
        );
    }
}
