use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use forge_domain::CommandResult;
use forge_domain::inline_shell::{InlineShellCommand, InlineShellError};

use super::{PromptProcessor, SecurityContext};
use crate::inline_shell::InlineShellExecutor;

/// Mock implementation of InlineShellExecutor for testing
pub struct MockInlineShellExecutor {
    results: Arc<std::sync::Mutex<HashMap<String, String>>>,
}

impl MockInlineShellExecutor {
    /// Create a new mock executor
    pub fn new() -> Self {
        Self { results: Arc::new(std::sync::Mutex::new(HashMap::new())) }
    }

    /// Add a predefined result for a command
    pub fn add_command_result(&self, command: String, output: String) {
        let mut results = self.results.lock().unwrap();
        results.insert(command, output);
    }

    /// Clear all predefined results
    pub fn clear_results(&self) {
        let mut results = self.results.lock().unwrap();
        results.clear();
    }
}

impl Default for MockInlineShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl InlineShellExecutor for MockInlineShellExecutor {
    async fn execute_commands(
        &self,
        commands: Vec<InlineShellCommand>,
        _working_dir: &std::path::Path,
        _restricted: bool,
    ) -> Result<Vec<CommandResult>, InlineShellError> {
        let results = self.results.lock().unwrap();
        let mut command_results = Vec::new();

        for command in commands {
            let output = results.get(&command.command).cloned();
            command_results.push(CommandResult {
                original_match: command.full_match.clone(), /* Use the original full_match from
                                                             * parser */
                command: command.command.clone(),
                stdout: output.unwrap_or_default(),
                stderr: String::new(),
                exit_code: 0,
            });
        }

        Ok(command_results)
    }
}

/// Mock implementation of PromptProcessor for testing
pub struct MockPromptProcessor {
    results: Arc<std::sync::Mutex<HashMap<String, String>>>,
}

impl MockPromptProcessor {
    /// Create a new mock prompt processor
    pub fn new() -> Self {
        Self { results: Arc::new(std::sync::Mutex::new(HashMap::new())) }
    }

    /// Add a predefined result for content
    pub fn add_result(&self, content: String, output: String) {
        let mut results = self.results.lock().unwrap();
        results.insert(content, output);
    }

    /// Clear all predefined results
    pub fn clear_results(&self) {
        let mut results = self.results.lock().unwrap();
        results.clear();
    }
}

impl Default for MockPromptProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PromptProcessor for MockPromptProcessor {
    async fn process_inline_commands(
        &self,
        content: &str,
        _security_context: &SecurityContext,
    ) -> Result<String> {
        let results = self.results.lock().unwrap();

        // Return predefined result if available, otherwise return original content
        if let Some(output) = results.get(content) {
            Ok(output.clone())
        } else {
            Ok(content.to_string())
        }
    }
}
