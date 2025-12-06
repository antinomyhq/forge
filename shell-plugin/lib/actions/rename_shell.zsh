use anyhow::Result;
use forge_select::ForgeSelect;

/// Shell action handler for conversation renaming
pub struct ShellRenameHandler;

impl ShellRenameHandler {
    /// Handle :rename command from shell plugin
    pub async fn handle_rename_command(conversation_id: &str) -> Result<()> {
        // Execute forge conversation list to get conversation info
        let output = std::process::Command::new("forge")
            .args(["conversation", "info", conversation_id])
            .output()
            .context("Failed to get conversation info")?;

        if !output.status.success() {
            anyhow::bail!("Failed to get conversation info");
        }

        // Extract current title from output
        let info_output = String::from_utf8_lossy(&output.stdout);
        let current_title = Self::extract_title(&info_output)
            .unwrap_or_else(|| "<untitled>".to_string());

        // Prompt for new title
        let new_title = Self::prompt_for_new_title(&current_title).await?;

        // Execute rename command
        let rename_output = std::process::Command::new("forge")
            .args(["conversation", "rename", conversation_id, &new_title])
            .output()
            .context("Failed to rename conversation")?;

        if !rename_output.status.success() {
            anyhow::bail!("Failed to rename conversation");
        }

        // Show success message
        let rename_stdout = String::from_utf8_lossy(&rename_output.stdout);
        if let Some(extracted_title) = Self::extract_success_title(&rename_stdout) {
            println!("✓ Conversation renamed to: {}", extracted_title);
        } else {
            println!("✓ Conversation renamed successfully");
        }

        Ok(())
    }

    /// Extract title from conversation info output
    fn extract_title(info_output: &str) -> Option<String> {
        info_output
            .lines()
            .find(|line| line.to_lowercase().contains("title:"))
            .and_then(|line| {
                line.split(':').nth(1)
                    .map(|title| title.trim().to_string())
            })
    }

    /// Extract success title from rename output
    fn extract_success_title(rename_output: &str) -> Option<String> {
        rename_output
            .lines()
            .find(|line| line.contains("renamed to"))
            .and_then(|line| {
                line.split("renamed to").nth(1)
                    .map(|title| title.trim().trim_matches('\'').to_string())
            })
    }

    /// Prompt user for new conversation title interactively
    async fn prompt_for_new_title(current_title: &str) -> Result<String> {
        let new_title = ForgeSelect::input(format!("Rename '{}' to:", current_title))
            .with_default(current_title)
            .prompt()?
            .context("Rename cancelled")?;

        if new_title.trim().is_empty() {
            anyhow::bail!("Title cannot be empty");
        }

        if new_title == current_title {
            anyhow::bail!("Title unchanged");
        }

        Ok(new_title.trim().to_string())
    }
}