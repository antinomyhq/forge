use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::*;

use crate::{
    AgentRegistry, AppConfigService, EnvironmentService, ProviderService, Services, ShellService,
    TemplateService,
};

/// Errors specific to GitApp operations
#[derive(thiserror::Error, Debug)]
pub enum GitAppError {
    #[error("nothing to commit, working tree clean")]
    NoChangesToCommit,
}

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
    /// Whether there are staged files (used internally)
    pub has_staged_files: bool,
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

    /// Generates a commit message without committing
    ///
    /// # Arguments
    ///
    /// * `max_diff_size` - Maximum size of git diff in bytes. None for
    ///   unlimited.
    ///
    /// # Errors
    ///
    /// Returns an error if git operations fail or AI generation fails
    pub async fn commit_message(&self, max_diff_size: Option<usize>) -> Result<CommitResult> {
        let CommitMessageDetails { message, has_staged_files } =
            self.generate_commit_message(max_diff_size).await?;

        Ok(CommitResult { message, committed: false, has_staged_files })
    }

    /// Commits changes with the provided commit message
    ///
    /// # Arguments
    ///
    /// * `message` - The commit message to use
    /// * `has_staged_files` - Whether there are staged files
    ///
    /// # Errors
    ///
    /// Returns an error if git commit fails
    pub async fn commit(&self, message: String, has_staged_files: bool) -> Result<CommitResult> {
        let cwd = self.services.environment_service().get_environment().cwd;

        let flags = if has_staged_files { "" } else { " -a" };
        let commit_command = format!("git commit {flags} -m '{message}'");

        let commit_result = self
            .services
            .shell_service()
            .execute(commit_command, cwd, false, false, None)
            .await
            .context("Failed to commit changes")?;

        if !commit_result.output.success() {
            anyhow::bail!("Git commit failed: {}", commit_result.output.stderr);
        }

        Ok(CommitResult { message, committed: true, has_staged_files })
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
            return Err(GitAppError::NoChangesToCommit.into());
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

        // Get required services and data in parallel
        let agent_id = self.services.get_active_agent_id().await?;
        let (rendered_prompt, provider, model) = tokio::try_join!(
            self.services
                .template_service()
                .render_template(Template::new("{{> forge-commit-message-prompt.md }}"), &()),
            self.get_provider(agent_id.clone()),
            self.get_model(agent_id)
        )?;

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

        Ok(CommitMessageDetails { message: Self::extract_commit_message(&message.content), has_staged_files })
    }

    /// Extracts the commit message from the AI response
    fn extract_commit_message(s: &str) -> String {
        let re = regex::Regex::new(r"^```[^\n]*\n?|```$").unwrap();
        re.replace_all(s.trim(), "").trim().to_string()
    }

    pub async fn get_provider(&self, agent: Option<AgentId>) -> anyhow::Result<Provider> {
        if let Some(agent) = agent
            && let Some(agent) = self.services.get_agent(&agent).await?
            && let Some(provider_id) = agent.provider
        {
            return self.services.get_provider(provider_id).await;
        }

        // Fall back to original logic if there is no agent
        // set yet.
        self.services.get_default_provider().await
    }

    /// Gets the model for the specified agent, or the default model if no agent
    /// is provided
    pub async fn get_model(&self, agent_id: Option<AgentId>) -> anyhow::Result<ModelId> {
        let provider_id = self.get_provider(agent_id).await?.id;
        self.services.get_default_model(&provider_id).await
    }
}
