use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_domain::{CommandOutput, Environment};
use forge_services::{CommandInfra, UserInfra};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;

/// Service for executing shell commands
#[derive(Clone, Debug)]
pub struct ForgeCommandExecutorService<U> {
    restricted: bool,
    env: Environment,
    user_infra: Arc<U>,

    // Mutex to ensure that only one command is executed at a time
    ready: Arc<Mutex<()>>,
}

impl<U: UserInfra> ForgeCommandExecutorService<U> {
    pub fn new(restricted: bool, env: Environment, user_infra: Arc<U>) -> Self {
        Self { restricted, env, user_infra, ready: Arc::new(Mutex::new(())) }
    }

    fn prepare_command(&self, command_str: &str, working_dir: Option<&Path>) -> Command {
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
        command.arg(parameter);

        #[cfg(windows)]
        command.raw_arg(command_str);
        #[cfg(unix)]
        command.arg(command_str);

        tracing::info!(command = command_str, "Executing command");

        command.kill_on_drop(true);

        // Set the working directory
        if let Some(working_dir) = working_dir {
            command.current_dir(working_dir);
        }

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
        // Ask for user confirmation before executing the command
        let confirmation_message = format!("Execute shell command: '{command}'?");
        let options = vec!["Yes".to_string(), "No".to_string()];

        match self
            .user_infra
            .select_one(&confirmation_message, options)
            .await?
        {
            Some(choice) if choice == "Yes" => {
                // User confirmed, proceed with execution
            }
            _ => {
                // User rejected or cancelled, return an error
                return Err(anyhow::anyhow!("Shell command execution cancelled by user"));
            }
        }

        let ready = self.ready.lock().await;

        let mut prepared_command = self.prepare_command(&command, Some(working_dir));

        // Spawn the command
        let mut child = prepared_command.spawn()?;

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
        drop(ready);

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout_buffer).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_buffer).into_owned(),
            exit_code: status.code(),
            command,
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
impl<U: UserInfra> CommandInfra for ForgeCommandExecutorService<U> {
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.execute_command_internal(command, &working_dir).await
    }

    async fn execute_command_raw(&self, command: &str) -> anyhow::Result<std::process::ExitStatus> {
        // Ask for user confirmation before executing the command
        let confirmation_message = format!("Execute shell command: '{command}'?");
        let options = vec!["Yes".to_string(), "No".to_string()];

        match self
            .user_infra
            .select_one(&confirmation_message, options)
            .await?
        {
            Some(choice) if choice == "Yes" => {
                // User confirmed, proceed with execution
            }
            _ => {
                // User rejected or cancelled, return an error
                return Err(anyhow::anyhow!("Shell command execution cancelled by user"));
            }
        }

        let mut prepared_command = self.prepare_command(command, None);

        // overwrite the stdin, stdout and stderr to inherit
        prepared_command
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        Ok(prepared_command.spawn()?.wait().await?)
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
            shell: if cfg!(target_os = "windows") {
                "cmd"
            } else {
                "bash"
            }
            .to_string(),
            base_path: PathBuf::from("/base"),
            provider: Provider::open_router("test-key"),
            retry_config: Default::default(),
            fetch_truncation_limit: 0,
            stdout_max_prefix_length: 0,
            max_search_lines: 0,
            max_read_size: 0,
            stdout_max_suffix_length: 0,
            http: Default::default(),
            max_file_size: 10_000_000,
        }
    }

    #[tokio::test]
    async fn test_command_executor() {
        // Create a mock user infra that always says "Yes"
        struct MockUserInfra;

        #[async_trait::async_trait]
        impl UserInfra for MockUserInfra {
            async fn prompt_question(&self, _question: &str) -> anyhow::Result<Option<String>> {
                Ok(Some("yes".to_string()))
            }

            async fn select_one(
                &self,
                _message: &str,
                options: Vec<String>,
            ) -> anyhow::Result<Option<String>> {
                // Always select the first option (which should be "Yes")
                Ok(options.first().cloned())
            }

            async fn select_many(
                &self,
                _message: &str,
                options: Vec<String>,
            ) -> anyhow::Result<Option<Vec<String>>> {
                Ok(Some(options))
            }
        }

        let fixture = ForgeCommandExecutorService::new(false, test_env(), Arc::new(MockUserInfra));
        let cmd = "echo 'hello world'";
        let dir = ".";

        let actual = fixture
            .execute_command(cmd.to_string(), PathBuf::new().join(dir))
            .await
            .unwrap();

        let mut expected = CommandOutput {
            stdout: "hello world\n".to_string(),
            stderr: "".to_string(),
            command: "echo \"hello world\"".into(),
            exit_code: Some(0),
        };

        if cfg!(target_os = "windows") {
            expected.stdout = format!("'{}'", expected.stdout);
        }

        assert_eq!(actual.stdout.trim(), expected.stdout.trim());
        assert_eq!(actual.stderr, expected.stderr);
        assert_eq!(actual.success(), expected.success());
    }
}
