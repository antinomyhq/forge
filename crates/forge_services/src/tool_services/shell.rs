use std::path::PathBuf;
use std::sync::Arc;

use anyhow::bail;
use forge_app::domain::Environment;
use forge_app::{ServiceContext, ShellOutput, ShellService};
use forge_domain::PolicyEngine;
use strip_ansi_escapes::strip;

use crate::{CommandInfra, EnvironmentInfra};

// Strips out the ansi codes from content.
fn strip_ansi(content: String) -> String {
    String::from_utf8_lossy(&strip(content.as_bytes())).into_owned()
}

/// Executes shell commands with safety measures using restricted bash (rbash).
/// Prevents potentially harmful operations like absolute path execution and
/// directory changes. Use for file system interaction, running utilities,
/// installing packages, or executing build commands. For operations requiring
/// unrestricted access, advise users to run forge CLI with '-u' flag. Returns
/// complete output including stdout, stderr, and exit code for diagnostic
/// purposes.
pub struct ForgeShell<I> {
    env: Environment,
    infra: Arc<I>,
}

impl<I: EnvironmentInfra> ForgeShell<I> {
    /// Create a new Shell with environment configuration
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        Self { env, infra }
    }

    fn validate_command(command: &str) -> anyhow::Result<()> {
        if command.trim().is_empty() {
            bail!("Command string is empty or contains only whitespace");
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<I: CommandInfra + EnvironmentInfra> ShellService for ForgeShell<I> {
    async fn execute(
        &self,
        command: String,
        cwd: PathBuf,
        keep_ansi: bool,
        context: &ServiceContext<'_>,
    ) -> anyhow::Result<ShellOutput> {
        let workflow = context.workflow;
        Self::validate_command(&command)?;

        let engine = PolicyEngine::new(workflow);
        let permission_trace = engine.can_execute(&command);

        // Check permission and handle according to policy
        match permission_trace.value {
            forge_domain::Permission::Disallow => {
                return Err(anyhow::anyhow!(
                    "Operation denied by policy at {}:{}. Execute access to '{}' is not permitted.",
                    permission_trace
                        .file
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    permission_trace.line.unwrap_or(0),
                    command
                ));
            }
            forge_domain::Permission::Allow => {
                // Continue with the operation
            }
            forge_domain::Permission::Confirm => {
                // Request user confirmation
                match context.request_confirmation() {
                    forge_domain::UserResponse::Accept
                    | forge_domain::UserResponse::AcceptAndRemember => {
                        // User accepted the operation, continue
                    }
                    forge_domain::UserResponse::Reject => {
                        return Err(anyhow::anyhow!(
                            "Operation rejected by user for command: {}",
                            command
                        ));
                    }
                }
            }
        }

        let mut output = self.infra.execute_command(command, cwd).await?;

        if !keep_ansi {
            output.stdout = strip_ansi(output.stdout);
            output.stderr = strip_ansi(output.stderr);
        }

        Ok(ShellOutput { output, shell: self.env.shell.clone() })
    }
}
