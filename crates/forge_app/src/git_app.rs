use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::*;

use crate::{
    EnvironmentService, ProviderRegistry, ProviderService, Services, ShellService, TemplateService,
};

/// GitApp handles git-related operations like commit message generation.
pub struct GitApp<S> {
    services: Arc<S>,
}

/// Result of a commit operation
#[derive(Debug, Clone)]
pub struct CommitResult {
    /// The generated commit message
    pub message: String,
    /// Whether the commit was actually executed (false for preview mode)
    pub committed: bool,
}

/// Details about commit message generation
#[derive(Debug, Clone)]
struct CommitMessageDetails {
    /// The generated commit message
    message: String,
    /// Whether there are staged files
    has_staged_files: bool,
}

impl<S: Services> GitApp<S> {
    /// Creates a new GitApp instance with the provided services.
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Commits changes with an AI-generated commit message
    ///
    /// # Arguments
    ///
    /// * `preview` - If true, only generates the message without committing
    /// * `max_diff_size` - Maximum size of git diff in bytes. None for
    ///   unlimited.
    ///
    /// # Errors
    ///
    /// Returns an error if git operations fail or AI generation fails
    pub async fn commit(
        &self,
        preview: bool,
        max_diff_size: Option<usize>,
    ) -> Result<CommitResult> {
        let CommitMessageDetails { message, has_staged_files } =
            self.generate_commit_message(max_diff_size).await?;

        if preview {
            return Ok(CommitResult { message, committed: false });
        }

        let cwd = self.services.environment_service().get_environment().cwd;

        // Execute the commit
        let escaped_message = message.replace('\'', "'\\''");
        let commit_command = if has_staged_files {
            format!("git commit -m '{escaped_message}'")
        } else {
            format!("git commit -a -m '{escaped_message}'")
        };

        let commit_result = self
            .services
            .shell_service()
            .execute(commit_command, cwd, false, false, None)
            .await
            .context("Failed to commit changes")?;

        if !commit_result.output.success() {
            anyhow::bail!("Git commit failed: {}", commit_result.output.stderr);
        }

        Ok(CommitResult { message, committed: true })
    }

    /// Generates a commit message based on staged git changes and returns
    /// details about the commit context
    async fn generate_commit_message(
        &self,
        max_diff_size: Option<usize>,
    ) -> Result<CommitMessageDetails> {
        // Get current working directory
        let cwd = self.services.environment_service().get_environment().cwd;

        // Execute git operations in parallel
        let (recent_commits, branch_name, staged_diff, unstaged_diff) = tokio::join!(
            self.services.shell_service().execute(
                "git log --pretty=format:%s --abbrev-commit --max-count=20".into(),
                cwd.clone(),
                false,
                true,
                None,
            ),
            self.services.shell_service().execute(
                "git rev-parse --abbrev-ref HEAD".into(),
                cwd.clone(),
                false,
                true,
                None,
            ),
            self.services.shell_service().execute(
                "git diff --staged".into(),
                cwd.clone(),
                false,
                true,
                None,
            ),
            self.services.shell_service().execute(
                "git diff".into(),
                cwd.clone(),
                false,
                true,
                None,
            )
        );

        let recent_commits = recent_commits.context("Failed to get recent commits")?;
        let branch_name = branch_name.context("Failed to get branch name")?;
        let staged_diff = staged_diff.context("Failed to get staged changes")?;
        let unstaged_diff = unstaged_diff.context("Failed to get unstaged changes")?;

        // Use staged changes if available, otherwise fall back to unstaged changes
        let has_staged_files = !staged_diff.output.stdout.trim().is_empty();
        let diff_output = if has_staged_files {
            staged_diff
        } else if !unstaged_diff.output.stdout.trim().is_empty() {
            unstaged_diff
        } else {
            return Err(anyhow::anyhow!("No changes to commit"));
        };

        // Truncate diff if it exceeds max size
        let (diff_content, was_truncated) = match max_diff_size {
            Some(max_size) if diff_output.output.stdout.len() > max_size => {
                // Safely truncate at a char boundary
                let truncated = diff_output
                    .output
                    .stdout
                    .char_indices()
                    .take_while(|(idx, _)| *idx < max_size)
                    .map(|(_, c)| c)
                    .collect::<String>();
                (truncated, true)
            }
            _ => (diff_output.output.stdout.clone(), false),
        };

        // Execute independent operations in parallel
        let (rendered_prompt, provider, model) = tokio::join!(
            self.services
                .template_service()
                .render_template("{{> forge-commit-message-prompt.md }}", &()),
            self.services.get_active_provider(),
            self.services.get_active_model()
        );

        let rendered_prompt = rendered_prompt?;
        let provider = provider.context("Failed to get provider")?;
        let model = model?;

        // Create an context
        let truncation_notice = if was_truncated {
            format!(
                "\n\n[Note: Diff truncated to {} bytes. Original size: {} bytes]",
                max_diff_size.unwrap(),
                diff_output.output.stdout.len()
            )
        } else {
            String::new()
        };

        let ctx = forge_domain::Context::default()
            .add_message(ContextMessage::system(rendered_prompt))
            .add_message(ContextMessage::user(
                format!(
                    "<branch_name>\n{}\n</branch_name>\n\n<recent_commit_messages>\n{}\n</recent_commit_messages>\n\n<git_diff>\n{}{}\n</git_diff>",
                    branch_name.output.stdout,
                    recent_commits.output.stdout,
                    diff_content,
                    truncation_notice
                ),
                Some(model.clone()),
            ));

        // Send message to LLM
        let stream = self
            .services
            .provider_service()
            .chat(&model, ctx, provider)
            .await?;
        let message = stream.into_full(false).await?;

        Ok(CommitMessageDetails { message: message.content, has_staged_files })
    }
}
