use std::sync::Arc;

use anyhow::{Context, Result};
use forge_api::API;

use crate::cli::CommitCommandGroup;

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
        let commit_message = self.api.generate_commit_message().await?;
        if args.preview {
            // Just return the message for preview
            Ok(commit_message)
        } else {
            // Actually commit the changes
            let escaped_message = commit_message.replace('\'', "'\\''");
            let cwd = self.api.environment().cwd;
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
