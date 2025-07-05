use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use forge_app::{AppConfig, ChoiceType};
use forge_domain::{CommandOutput, Environment};
use forge_services::{
    CommandExecutionPrompt, CommandInfra, FileReaderInfra, FileWriterInfra, UserInfra,
};
use strum::IntoEnumIterator;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;

/// Service for executing shell commands
#[derive(Clone, Debug)]
pub struct ForgeCommandExecutorService<U, F, R> {
    restricted: bool,
    env: Environment,
    user_infra: Arc<U>,
    file_writer_infra: Arc<F>,
    file_reader_infra: Arc<R>,

    // Mutex to ensure that only one command is executed at a time
    ready: Arc<Mutex<()>>,
}

impl<U: UserInfra, F: FileWriterInfra, R: FileReaderInfra> ForgeCommandExecutorService<U, F, R> {
    pub fn new(
        restricted: bool,
        env: Environment,
        user_infra: Arc<U>,
        file_writer_infra: Arc<F>,
        file_reader_infra: Arc<R>,
    ) -> Self {
        Self {
            restricted,
            env,
            user_infra,
            file_writer_infra,
            file_reader_infra,
            ready: Arc::new(Mutex::new(())),
        }
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
        self.confirm_execution(&command).await?;

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

    async fn confirm_execution(&self, command: &str) -> anyhow::Result<()> {
        let config_path = self.env.app_config();

        if let Ok(content) = self.file_reader_infra.read_utf8(&config_path).await {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&content) {
                if let Some(choices) = &config.choices {
                    if let Some(execute_shell_commands) = &choices.execute_shell_commands {
                        match execute_shell_commands {
                            ChoiceType::Allow => return Ok(()),
                            ChoiceType::Reject => {
                                return Err(anyhow::anyhow!(
                                    "Shell command execution disabled by saved preference"
                                ))
                            }
                            ChoiceType::AskEveryTime => {}
                        }
                    }
                }
            }
        }

        let confirmation_message = format!("Execute shell command: '{command}'?");

        match self
            .user_infra
            .select_one(
                &confirmation_message,
                CommandExecutionPrompt::iter().collect(),
            )
            .await?
        {
            Some(choice) if choice.is_remember() => {
                self.save_shell_execution_config(choice.is_accept()).await?;
                if choice.is_accept() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Shell command execution cancelled by user"))
                }
            }
            Some(choice) if choice.is_accept() => Ok(()),
            _ => Err(anyhow::anyhow!("Shell command execution cancelled by user")),
        }
    }

    /// Save the user's shell execution preference to config
    async fn save_shell_execution_config(&self, accept: bool) -> anyhow::Result<()> {
        let config_path = self.env.app_config();

        let content = self
            .file_reader_infra
            .read_utf8(&config_path)
            .await
            .unwrap_or_default();
        let mut config = serde_json::from_str::<AppConfig>(&content).unwrap_or_default();

        config.choices = Some(config.choices.unwrap_or_default());

        if let Some(ref mut choices) = config.choices {
            choices.execute_shell_commands = Some(if accept {
                ChoiceType::Allow
            } else {
                ChoiceType::Reject
            });
        }

        let content = serde_json::to_string(&config)?;
        self.file_writer_infra
            .write(&config_path, Bytes::from(content), false)
            .await?;

        Ok(())
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
impl<U: UserInfra, F: FileWriterInfra, R: FileReaderInfra> CommandInfra
    for ForgeCommandExecutorService<U, F, R>
{
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.execute_command_internal(command, &working_dir).await
    }

    async fn execute_command_raw(&self, command: &str) -> anyhow::Result<std::process::ExitStatus> {
        // Ask for user confirmation before executing the command
        self.confirm_execution(command).await?;

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
    use std::fmt::Display;

    use forge_fs::FileInfo;
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

    // Create a mock user infra that always says "Yes"
    struct MockUserInfra;

    #[async_trait::async_trait]
    impl UserInfra for MockUserInfra {
        async fn prompt_question(&self, _question: &str) -> anyhow::Result<Option<String>> {
            Ok(Some("yes".to_string()))
        }

        async fn select_one<T: Display + Send + Clone + 'static>(
            &self,
            _message: &str,
            options: Vec<T>,
        ) -> anyhow::Result<Option<T>> {
            // Always select the first option (which should be "Yes")
            Ok(options.first().cloned())
        }

        async fn select_many<T: Display + Send + Clone + 'static>(
            &self,
            _message: &str,
            options: Vec<T>,
        ) -> anyhow::Result<Option<Vec<T>>> {
            Ok(Some(options))
        }
    }

    // Create a mock file writer that does nothing
    struct MockFileWriter;

    #[async_trait::async_trait]
    impl FileWriterInfra for MockFileWriter {
        async fn write(
            &self,
            _path: &std::path::Path,
            _contents: Bytes,
            _capture_snapshot: bool,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn write_temp(
            &self,
            _prefix: &str,
            _ext: &str,
            _content: &str,
        ) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/tmp/test"))
        }
    }

    // Create a mock file reader that returns empty config
    struct MockFileReader;

    #[async_trait::async_trait]
    impl FileReaderInfra for MockFileReader {
        async fn read_utf8(&self, _path: &std::path::Path) -> anyhow::Result<String> {
            Ok("{}".to_string()) // Return empty JSON config
        }

        async fn read(&self, _path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
            Ok(b"{}".to_vec())
        }

        async fn range_read_utf8(
            &self,
            _path: &std::path::Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, FileInfo)> {
            Ok((
                "{}".to_string(),
                FileInfo { start_line: 1, end_line: 1, total_lines: 1 },
            ))
        }
    }

    // Create a mock file reader that returns config with execute_shell_commands:
    // true
    struct MockFileReaderWithAutoExecute;

    #[async_trait::async_trait]
    impl FileReaderInfra for MockFileReaderWithAutoExecute {
        async fn read_utf8(&self, _path: &std::path::Path) -> anyhow::Result<String> {
            Ok(r#"{"choices":{"executeShellCommands":"allow"}}"#.to_string())
        }

        async fn read(&self, _path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
            Ok(br#"{"choices":{"executeShellCommands":"allow"}}"#.to_vec())
        }

        async fn range_read_utf8(
            &self,
            _path: &std::path::Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, FileInfo)> {
            Ok((
                r#"{"choices":{"executeShellCommands":"allow"}}"#.to_string(),
                FileInfo { start_line: 1, end_line: 1, total_lines: 1 },
            ))
        }
    }

    #[tokio::test]
    async fn test_confirm_execution_with_auto_execute_config() {
        // Create a mock user infra that should never be called
        struct MockUserInfraNotCalled;

        #[async_trait::async_trait]
        impl UserInfra for MockUserInfraNotCalled {
            async fn prompt_question(&self, _question: &str) -> anyhow::Result<Option<String>> {
                panic!("User should not be prompted when auto-execute is enabled")
            }

            async fn select_one<T: Display + Send + Clone + 'static>(
                &self,
                _message: &str,
                _options: Vec<T>,
            ) -> anyhow::Result<Option<T>> {
                panic!("User should not be prompted when auto-execute is enabled")
            }

            async fn select_many<T: Display + Send + Clone + 'static>(
                &self,
                _message: &str,
                _options: Vec<T>,
            ) -> anyhow::Result<Option<Vec<T>>> {
                panic!("User should not be prompted when auto-execute is enabled")
            }
        }

        let fixture = ForgeCommandExecutorService::new(
            false,
            test_env(),
            Arc::new(MockUserInfraNotCalled),
            Arc::new(MockFileWriter),
            Arc::new(MockFileReaderWithAutoExecute),
        );

        // Test that confirm_execution returns Ok without prompting
        let result = fixture.confirm_execution("echo test").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_command_executor() {
        let fixture = ForgeCommandExecutorService::new(
            false,
            test_env(),
            Arc::new(MockUserInfra),
            Arc::new(MockFileWriter),
            Arc::new(MockFileReader),
        );
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

    #[tokio::test]
    async fn test_command_executor_with_auto_execute() {
        // Create a mock user infra that should never be called
        struct MockUserInfraNotCalled;

        #[async_trait::async_trait]
        impl UserInfra for MockUserInfraNotCalled {
            async fn prompt_question(&self, _question: &str) -> anyhow::Result<Option<String>> {
                panic!("User should not be prompted when auto-execute is enabled")
            }

            async fn select_one<T: Display + Send + Clone + 'static>(
                &self,
                _message: &str,
                _options: Vec<T>,
            ) -> anyhow::Result<Option<T>> {
                panic!("User should not be prompted when auto-execute is enabled")
            }

            async fn select_many<T: Display + Send + Clone + 'static>(
                &self,
                _message: &str,
                _options: Vec<T>,
            ) -> anyhow::Result<Option<Vec<T>>> {
                panic!("User should not be prompted when auto-execute is enabled")
            }
        }

        let fixture = ForgeCommandExecutorService::new(
            false,
            test_env(),
            Arc::new(MockUserInfraNotCalled),
            Arc::new(MockFileWriter),
            Arc::new(MockFileReaderWithAutoExecute),
        );
        let cmd = "echo 'auto execute test'";
        let dir = ".";

        let actual = fixture
            .execute_command(cmd.to_string(), PathBuf::new().join(dir))
            .await
            .unwrap();

        let mut expected = CommandOutput {
            stdout: "auto execute test\n".to_string(),
            stderr: "".to_string(),
            command: "echo \"auto execute test\"".into(),
            exit_code: Some(0),
        };

        if cfg!(target_os = "windows") {
            expected.stdout = format!("'{}'", expected.stdout);
        }

        assert_eq!(actual.stdout.trim(), expected.stdout.trim());
        assert_eq!(actual.stderr, expected.stderr);
        assert_eq!(actual.success(), expected.success());
    }

    // Create a mock file reader that returns config with execute_shell_commands:
    // Reject
    struct MockFileReaderWithAutoReject;

    #[async_trait::async_trait]
    impl FileReaderInfra for MockFileReaderWithAutoReject {
        async fn read_utf8(&self, _path: &std::path::Path) -> anyhow::Result<String> {
            Ok(r#"{"choices":{"executeShellCommands":"reject"}}"#.to_string())
        }

        async fn read(&self, _path: &std::path::Path) -> anyhow::Result<Vec<u8>> {
            Ok(br#"{"choices":{"executeShellCommands":"reject"}}"#.to_vec())
        }

        async fn range_read_utf8(
            &self,
            _path: &std::path::Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, FileInfo)> {
            Ok((
                r#"{"choices":{"executeShellCommands":"reject"}}"#.to_string(),
                FileInfo { start_line: 1, end_line: 1, total_lines: 1 },
            ))
        }
    }

    #[tokio::test]
    async fn test_confirm_execution_with_auto_reject_config() {
        // Create a mock user infra that should never be called
        struct MockUserInfraNotCalled;

        #[async_trait::async_trait]
        impl UserInfra for MockUserInfraNotCalled {
            async fn prompt_question(&self, _question: &str) -> anyhow::Result<Option<String>> {
                panic!("User should not be prompted when auto-reject is enabled")
            }

            async fn select_one<T: Display + Send + Clone + 'static>(
                &self,
                _message: &str,
                _options: Vec<T>,
            ) -> anyhow::Result<Option<T>> {
                panic!("User should not be prompted when auto-reject is enabled")
            }

            async fn select_many<T: Display + Send + Clone + 'static>(
                &self,
                _message: &str,
                _options: Vec<T>,
            ) -> anyhow::Result<Option<Vec<T>>> {
                panic!("User should not be prompted when auto-reject is enabled")
            }
        }

        let fixture = ForgeCommandExecutorService::new(
            false,
            test_env(),
            Arc::new(MockUserInfraNotCalled),
            Arc::new(MockFileWriter),
            Arc::new(MockFileReaderWithAutoReject),
        );

        // Test that confirm_execution returns Err without prompting
        let result = fixture.confirm_execution("echo test").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("disabled by saved preference"));
    }
}
