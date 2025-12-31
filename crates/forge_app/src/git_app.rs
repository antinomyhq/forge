use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_domain::*;
use forge_template::Element;

use crate::{
    AgentProviderResolver, AgentRegistry, AppConfigService, EnvironmentService,
    ProviderAuthService, ProviderService, ShellService, TemplateService,
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

/// Context for generating a commit message from a diff
#[derive(Debug)]
struct DiffContext {
    diff_content: String,
    branch_name: String,
    recent_commits: String,
    has_staged_files: bool,
    additional_context: Option<String>,
}

impl<S> GitApp<S> {
    /// Creates a new GitApp instance with the provided services.
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Truncates diff content if it exceeds the maximum size
    fn truncate_diff(
        &self,
        diff_content: String,
        max_diff_size: Option<usize>,
        original_size: usize,
    ) -> (String, bool) {
        match max_diff_size {
            Some(max_size) if original_size > max_size => {
                // Safely truncate at a char boundary
                let truncated = diff_content
                    .char_indices()
                    .take_while(|(idx, _)| *idx < max_size)
                    .map(|(_, c)| c)
                    .collect::<String>();
                (truncated, true)
            }
            _ => (diff_content, false),
        }
    }
}

impl<S> GitApp<S>
where
    S: EnvironmentService
        + ShellService
        + AgentRegistry
        + TemplateService
        + ProviderService
        + AppConfigService
        + ProviderAuthService,
{
    /// Generates a commit message without committing
    ///
    /// # Arguments
    ///
    /// * `max_diff_size` - Maximum size of git diff in bytes. None for
    ///   unlimited.
    /// * `diff` - Optional diff content provided via pipe. If provided, this
    ///   diff is used instead of fetching from git.
    /// * `additional_context` - Optional additional text to help structure the
    ///   commit message
    ///
    /// # Errors
    ///
    /// Returns an error if git operations fail or AI generation fails
    pub async fn commit_message(
        &self,
        max_diff_size: Option<usize>,
        diff: Option<String>,
        additional_context: Option<String>,
    ) -> Result<CommitResult> {
        let CommitMessageDetails { message, has_staged_files } = self
            .generate_commit_message(max_diff_size, diff, additional_context)
            .await?;

        let message_with_trailers = self.add_coauthor_trailers(message).await;

        Ok(CommitResult {
            message: message_with_trailers,
            committed: false,
            has_staged_files,
        })
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
        let cwd = self.services.get_environment().cwd;
        let message_with_trailers = self.add_coauthor_trailers(message).await;

        let flags = if has_staged_files { "" } else { " -a" };
        let commit_command = format!("git commit {flags} -m '{message_with_trailers}'");

        let commit_result = self
            .services
            .execute(commit_command, cwd, false, false, None)
            .await
            .context("Failed to commit changes")?;

        if !commit_result.output.success() {
            anyhow::bail!("Git commit failed: {}", commit_result.output.stderr);
        }

        Ok(CommitResult {
            message: message_with_trailers,
            committed: true,
            has_staged_files,
        })
    }

    /// Adds co-authored-by trailers to a commit message
    ///
    /// If git user information cannot be retrieved, only the ForgeCode trailer
    /// is added. This method never fails.
    async fn add_coauthor_trailers(&self, message: String) -> String {
        let cwd = self.services.get_environment().cwd;
        let message = message.trim_end();
        
        match self.get_git_user_info(&cwd).await {
            Some((user_name, user_email)) => {
                format!(
                    "{}\n\nCo-Authored-By: {} <{}>\nCo-Authored-By: ForgeCode <noreply@forgecode.dev>",
                    message, user_name, user_email
                )
            }
            None => {
                format!(
                    "{}\n\nCo-Authored-By: ForgeCode <noreply@forgecode.dev>",
                    message
                )
            }
        }
    }

    /// Gets git user name and email from git config
    ///
    /// Returns None if git config is not set or git commands fail
    async fn get_git_user_info(&self, cwd: &Path) -> Option<(String, String)> {
        let (user_name_result, user_email_result) = tokio::join!(
            self.services.execute(
                "git config user.name".into(),
                cwd.to_path_buf(),
                false,
                true,
                None,
            ),
            self.services.execute(
                "git config user.email".into(),
                cwd.to_path_buf(),
                false,
                true,
                None,
            ),
        );

        let user_name = user_name_result.ok()?.output.stdout.trim().to_string();

        let user_email = user_email_result.ok()?.output.stdout.trim().to_string();

        if user_name.is_empty() || user_email.is_empty() {
            return None;
        }

        Some((user_name, user_email))
    }

    /// Generates a commit message based on staged git changes and returns
    /// details about the commit context
    async fn generate_commit_message(
        &self,
        max_diff_size: Option<usize>,
        diff: Option<String>,
        additional_context: Option<String>,
    ) -> Result<CommitMessageDetails> {
        // Get current working directory
        let cwd = self.services.get_environment().cwd;

        // Fetch git context (always needed for commit message generation)
        let (recent_commits, branch_name) = self.fetch_git_context(&cwd).await?;

        // Get diff content and metadata
        let (diff_content, original_size, has_staged_files) = if let Some(piped_diff) = diff {
            // Use piped diff
            let size = piped_diff.len();
            (piped_diff, size, false) // Assume unstaged for piped diff
        } else {
            // Fetch diff from git
            self.fetch_git_diff(&cwd).await?
        };

        // Truncate diff if it exceeds max size
        let (truncated_diff, _) = self.truncate_diff(diff_content, max_diff_size, original_size);

        self.generate_message_from_diff(DiffContext {
            diff_content: truncated_diff,
            branch_name,
            recent_commits,
            has_staged_files,
            additional_context,
        })
        .await
    }

    /// Fetches git context (branch name and recent commits)
    async fn fetch_git_context(&self, cwd: &Path) -> Result<(String, String)> {
        let (recent_commits, branch_name) = tokio::join!(
            self.services.execute(
                "git log --pretty=format:%s --abbrev-commit --max-count=20".into(),
                cwd.to_path_buf(),
                false,
                true,
                None,
            ),
            self.services.execute(
                "git rev-parse --abbrev-ref HEAD".into(),
                cwd.to_path_buf(),
                false,
                true,
                None,
            ),
        );

        let recent_commits = recent_commits.context("Failed to get recent commits")?;
        let branch_name = branch_name.context("Failed to get branch name")?;

        Ok((recent_commits.output.stdout, branch_name.output.stdout))
    }

    /// Fetches diff from git (staged or unstaged)
    async fn fetch_git_diff(&self, cwd: &Path) -> Result<(String, usize, bool)> {
        let (staged_diff, unstaged_diff) = tokio::join!(
            self.services.execute(
                "git diff --staged".into(),
                cwd.to_path_buf(),
                false,
                true,
                None,
            ),
            self.services
                .execute("git diff".into(), cwd.to_path_buf(), false, true, None,)
        );

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

        let size = diff_output.output.stdout.len();
        Ok((diff_output.output.stdout, size, has_staged_files))
    }

    /// Generates a commit message from the provided diff and git context
    async fn generate_message_from_diff(&self, ctx: DiffContext) -> Result<CommitMessageDetails> {
        // Get required services and data in parallel
        let agent_id = self.services.get_active_agent_id().await?;
        let agent_provider_resolver = AgentProviderResolver::new(self.services.clone());
        let (rendered_prompt, provider, model) = tokio::try_join!(
            self.services
                .render_template(Template::new("{{> forge-commit-message-prompt.md }}"), &()),
            agent_provider_resolver.get_provider(agent_id.clone()),
            agent_provider_resolver.get_model(agent_id)
        )?;
        let provider = self.services.refresh_provider_credential(provider).await?;
        // Build git diff content with optional truncation notice
        // Build user message using Element
        let mut user_message = Element::new("user_message")
            .append(Element::new("branch_name").text(&ctx.branch_name))
            .append(Element::new("recent_commit_messages").text(&ctx.recent_commits))
            .append(Element::new("git_diff").cdata(&ctx.diff_content));

        // Add additional context if provided
        if let Some(additional_context) = &ctx.additional_context {
            user_message =
                user_message.append(Element::new("additional_context").text(additional_context));
        }

        let context = forge_domain::Context::default()
            .add_message(ContextMessage::system(rendered_prompt))
            .add_message(ContextMessage::user(
                user_message.to_string(),
                Some(model.clone()),
            ));

        // Send message to LLM
        let stream = self.services.chat(&model, context, provider).await?;
        let message = stream.into_full(false).await?;

        // Extract the command from the <shell_command> tag
        let commit_message = forge_domain::extract_tag_content(&message.content, "commit_message")
            .ok_or_else(|| anyhow::anyhow!("Failed to generate commit message"))?;

        Ok(CommitMessageDetails {
            message: commit_message.to_string(),
            has_staged_files: ctx.has_staged_files,
        })
    }
}
