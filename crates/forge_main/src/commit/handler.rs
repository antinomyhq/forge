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
        let commit_message = self.api.generate_commit_message(args.max_diff_size).await?;
        if args.preview {
            // Just return the message for preview
            Ok(commit_message)
        } else {
            // Check if there are staged changes first
            let cwd = self.api.environment().cwd;
            let status_result = self
                .api
                .execute_shell_command("git diff --cached --quiet", cwd.clone(), false)
                .await
                .context("Failed to check staged changes")?;

            // If exit code is 0, there are no staged changes; use -a as fallback
            // If exit code is 1, there are staged changes; commit only those
            let staged_files = status_result.exit_code.unwrap_or_default() == 1;

            // Actually commit the changes
            let escaped_message = commit_message.replace('\'', "'\\''");
            let commit_command = if staged_files {
                format!("git commit -m '{escaped_message}'")
            } else {
                format!("git commit -a -m '{escaped_message}'")
            };

            let commit_result = self
                .api
                .execute_shell_command(&commit_command, cwd.clone(), false)
                .await
                .context("Failed to commit changes")?;

            if !commit_result.success() {
                anyhow::bail!("Git commit failed: {}", commit_result.stderr);
            }

            Ok(commit_message)
        }
    }
}
