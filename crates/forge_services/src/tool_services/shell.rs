use std::path::PathBuf;
use std::sync::Arc;

use anyhow::bail;
use forge_app::domain::Environment;
use forge_app::{ShellOutput, ShellService};
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
    ) -> anyhow::Result<ShellOutput> {
        Self::validate_command(&command)?;

        let mut output = self.infra.execute_command(command, cwd, false).await?;

        if !keep_ansi {
            output.stdout = strip_ansi(output.stdout);
            output.stderr = strip_ansi(output.stderr);
        }

        Ok(ShellOutput { output, shell: self.env.shell.clone() })
    }
}
