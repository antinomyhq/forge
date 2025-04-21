use std::io::{self, Write};
use std::path::{Path, PathBuf};

use forge_domain::{CommandOutput, Environment};
use forge_services::CommandExecutorService;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Service for executing shell commands
#[derive(Clone, Debug)]
pub struct ForgeCommandExecutorService {
    restricted: bool,
    env: Environment,
}

impl ForgeCommandExecutorService {
    pub fn new(restricted: bool, env: Environment) -> Self {
        Self { restricted, env }
    }

    fn prepare_command(&self, command_str: &str, working_dir: &Path) -> Command {
        // Create a basic command
        let is_windows = cfg!(target_os = "windows");
        let shell = if self.restricted && !is_windows {
            "rbash"
        } else {
            self.env.shell.as_str()
        };
        let mut command = Command::new(shell);

        // Core color settings for general commands
        command
            .env("CLICOLOR_FORCE", "1")
            .env("FORCE_COLOR", "true")
            .env_remove("NO_COLOR");

        // Language/program specific color settings
        command
            .env("SBT_OPTS", "-Dsbt.color=always")
            .env("JAVA_OPTS", "-Dsbt.color=always");

        // enabled Git colors
        command.env("GIT_CONFIG_PARAMETERS", "'color.ui=always'");

        // Other common tools
        command.env("GREP_OPTIONS", "--color=always"); // GNU grep

        let parameter = if is_windows { "/C" } else { "-c" };

        command.arg(parameter).arg(command_str);

        command.kill_on_drop(true);

        // Set the working directory
        command.current_dir(working_dir);

        // Configure the command for output
        command
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        command
    }

    /// Internal method to execute commands with streaming to console
    async fn execute_command_internal(
        &self,
        command: String,
        working_dir: &Path,
    ) -> anyhow::Result<CommandOutput> {
        let mut command = self.prepare_command(&command, working_dir);

        // Spawn the command
        let mut child = command.spawn()?;

        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();

        // Stream the output of the command to stdout and stderr concurrently
        let (status, stdout_buffer, stderr_buffer) = tokio::try_join!(
            child.wait(),
            stream(&mut stdout_pipe, io::stdout()),
            stream(&mut stderr_pipe, io::stderr())
        )?;

        // Drop happens after `try_join` due to <https://github.com/tokio-rs/tokio/issues/4309>
        drop(stdout_pipe);
        drop(stderr_pipe);

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout_buffer).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_buffer).into_owned(),
            success: status.success(),
        })
    }
}

/// reads the output from A and writes it to W
async fn stream<A: AsyncReadExt + Unpin, W: Write>(
    io: &mut Option<A>,
    mut writer: W,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    if let Some(io) = io.as_mut() {
        let mut buff = [0; 1024];
        loop {
            let n = io.read(&mut buff).await?;
            if n == 0 {
                break;
            }
            writer.write_all(&buff[..n])?;
            // note: flush is necessary else we get the cursor could not be found error.
            writer.flush()?;
            output.extend_from_slice(&buff[..n]);
        }
    }
    Ok(output)
}

/// The implementation for CommandExecutorService
#[async_trait::async_trait]
impl CommandExecutorService for ForgeCommandExecutorService {
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.execute_command_internal(command, &working_dir).await
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::Provider;
    use pretty_assertions::assert_eq;

    use super::*;

    fn test_env() -> Environment {
        Environment {
            os: "test".to_string(),
            pid: 12345,
            cwd: PathBuf::from("/test"),
            home: Some(PathBuf::from("/home/test")),
            shell: "bash".to_string(),
            base_path: PathBuf::from("/base"),
            provider: Provider::open_router("test-key"),
            retry_config: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_command_executor() {
        let fixture = ForgeCommandExecutorService::new(false, test_env());
        let cmd = "echo 'hello world'";
        let dir = ".";

        let actual = fixture
            .execute_command(cmd.to_string(), PathBuf::new().join(dir))
            .await
            .unwrap();

        let expected = CommandOutput {
            stdout: "hello world\n".to_string(),
            stderr: "".to_string(),
            success: true,
        };

        assert_eq!(actual.stdout.trim(), expected.stdout.trim());
        assert_eq!(actual.stderr, expected.stderr);
        assert_eq!(actual.success, expected.success);
    }
}
