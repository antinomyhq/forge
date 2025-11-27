use std::fmt::Display;

use anyhow::Result;
use colored::Colorize;
use forge_domain::ContextMessage;
use forge_select::ForgeSelect;

/// Logic for selecting prompts from a conversation
pub struct PromptSelector;

#[derive(Clone)]
struct PromptItem {
    index: usize,
    content: String,
    message: ContextMessage,
}

impl Display for PromptItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MAX_CONTENT: usize = 80;
        let truncated = if self.content.len() > MAX_CONTENT {
            format!(
                "{}...",
                self.content.chars().take(MAX_CONTENT).collect::<String>()
            )
        } else {
            self.content.clone()
        };
        write!(f, "{:3}. {}", self.index + 1, truncated.bold())
    }
}

impl PromptSelector {
    /// Select a prompt from the provided list of prompts with indices
    ///
    /// Returns the selected prompt index and message, or None if no selection
    /// was made
    pub async fn select_prompt(
        prompts: &[(usize, &ContextMessage)],
    ) -> Result<Option<(usize, ContextMessage)>> {
        if prompts.is_empty() {
            return Ok(None);
        }

        let items: Vec<PromptItem> = prompts
            .iter()
            .map(|(index, msg)| {
                let content = msg
                    .content()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                PromptItem { index: *index, content, message: (*msg).clone() }
            })
            .collect();

        if let Some(selected) = tokio::task::spawn_blocking(|| {
            ForgeSelect::select("Select a prompt to branch from:", items)
                .with_help_message(
                    "Type a number or use arrow keys to navigate and Enter to select",
                )
                .prompt()
        })
        .await??
        {
            Ok(Some((selected.index, selected.message)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{ContextMessage, ModelId};
    

    use super::*;

    fn create_test_prompt(index: usize, content: &str) -> (usize, ContextMessage) {
        (
            index,
            ContextMessage::user(content, Some(ModelId::new("test-model"))),
        )
    }

    #[test]
    fn test_prompt_item_display() {
        let message = ContextMessage::user("Test prompt", None);
        let item = PromptItem { index: 0, content: "Test prompt".to_string(), message };
        let display = format!("{}", item);
        assert!(display.contains("1."));
        assert!(display.contains("Test prompt"));
    }

    #[test]
    fn test_prompt_item_truncation() {
        let long_content = "a".repeat(100);
        let message = ContextMessage::user(&long_content, None);
        let item = PromptItem { index: 0, content: long_content.clone(), message };
        let display = format!("{}", item);
        assert!(display.len() < long_content.len() + 10); // Should be truncated
        assert!(display.contains("..."));
    }
}
