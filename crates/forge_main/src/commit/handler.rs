use std::sync::Arc;

use anyhow::{Context, Result};
use forge_api::{API, AgentId};
use forge_domain::{ChatRequest, ChatResponse, Conversation, Event};
use tokio_stream::StreamExt;

use crate::cli::CommitCommandGroup;

const COMMIT_MESSAGE_PROMPT: &str =
    include_str!("../../../../templates/forge-commit-message-prompt.md");

/// Handler for commit command operations
pub struct CommitHandler<A> {
    api: Arc<A>,
}

impl<A> CommitHandler<A> {
    /// Creates a new CommitHandler
    pub fn new(api: Arc<A>) -> Self {
        Self { api }
    }
}

impl<A: API> CommitHandler<A> {
    /// Handle commit command
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Git commands fail
    /// - AI generation fails
    /// - Commit operation fails
    pub async fn handle(&self, args: CommitCommandGroup) -> Result<String> {
        // Get current working directory
        let cwd = self.api.environment().cwd;

        // Get recent commit messages as examples
        let recent_commits = self
            .api
            .execute_shell_command(
                "git log --pretty=format:%s --abbrev-commit --max-count=20",
                cwd.clone(),
                true,
            )
            .await
            .context("Failed to get recent commits")?;

        // Get staged changes
        let diff_output = self
            .api
            .execute_shell_command("git diff --staged", cwd.clone(), true)
            .await
            .context("Failed to get staged changes")?;

        if diff_output.stdout.trim().is_empty() {
            return Err(anyhow::anyhow!("No staged changes to commit"));
        }

        // Build the prompt for AI
        let prompt = COMMIT_MESSAGE_PROMPT
            .replace("{examples}", &recent_commits.stdout)
            .replace("{diff}", &diff_output.stdout);

        // Generate commit message using the chat API with default agent.
        let event = Event::new(format!("{}", AgentId::default()), Some(prompt));

        // Create a new conversation for this commit message generation
        let mut conversation = Conversation::generate();
        // To avoid generating title.
        conversation.title = Some("Generating git commit message".into());
        let conversation_id = conversation.id;
        self.api.upsert_conversation(conversation).await?;

        let chat_request = ChatRequest::new(event, conversation_id);

        let mut stream = self.api.chat(chat_request).await?;
        let mut commit_message = String::new();

        while let Some(response) = stream.next().await {
            let response = response?;
            if let ChatResponse::TaskMessage { content } = response {
                commit_message.push_str(content.as_str());
            }
        }

        let commit_message = commit_message.trim().to_string();

        if args.preview {
            // Just return the message for preview
            Ok(commit_message)
        } else {
            // Actually commit the changes
            let escaped_message = commit_message.replace('\'', "'\\''");
            let commit_result = self
                .api
                .execute_shell_command(
                    &format!("git commit -m '{}'", escaped_message),
                    cwd.clone(),
                    false,
                )
                .await
                .context("Failed to commit changes")?;

            if !commit_result.success() {
                anyhow::bail!("Git commit failed: {}", commit_result.stderr);
            }

            Ok(format!(
                "Committed with message:\n{}\n\n{}",
                commit_message, commit_result.stdout
            ))
        }
    }
}
