use std::path::PathBuf;
use std::sync::Arc;

use anyhow::bail;
use forge_app::domain::Environment;
use forge_app::{CommandInfra, EnvironmentInfra, ShellOutput, ShellOutputKind, ShellService};

use super::BackgroundProcessManager;
use strip_ansi_escapes::strip;

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
    bg_manager: Arc<BackgroundProcessManager>,
}

impl<I: EnvironmentInfra> ForgeShell<I> {
    /// Create a new Shell with environment configuration and a background
    /// process manager for tracking long-running detached processes.
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.get_environment();
        let bg_manager = Arc::new(BackgroundProcessManager::new());
        Self { env, infra, bg_manager }
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
        silent: bool,
        background: bool,
        env_vars: Option<Vec<String>>,
        description: Option<String>,
    ) -> anyhow::Result<ShellOutput> {
        Self::validate_command(&command)?;

        if background {
            let bg_output = self
                .infra
                .execute_command_background(command, cwd.clone(), env_vars)
                .await?;

            // Register with the background process manager which takes
            // ownership of the temp-file handle (keeps the log file alive).
            self.bg_manager.register(
                bg_output.pid,
                bg_output.command.clone(),
                cwd,
                bg_output.log_file.clone(),
                bg_output.log_handle,
            )?;

            return Ok(ShellOutput {
                kind: ShellOutputKind::Background {
                    command: bg_output.command,
                    pid: bg_output.pid,
                    log_file: bg_output.log_file,
                },
                shell: self.env.shell.clone(),
                description,
            });
        }

        let mut output = self
            .infra
            .execute_command(command, cwd, silent, env_vars)
            .await?;

        if !keep_ansi {
            output.stdout = strip_ansi(output.stdout);
            output.stderr = strip_ansi(output.stderr);
        }

        Ok(ShellOutput {
            kind: ShellOutputKind::Foreground(output),
            shell: self.env.shell.clone(),
            description,
        })
    }

    fn list_background_processes(
        &self,
    ) -> anyhow::Result<Vec<(forge_domain::BackgroundProcess, bool)>> {
        self.bg_manager.list_with_status()
    }

    fn kill_background_process(&self, pid: u32, delete_log: bool) -> anyhow::Result<()> {
        self.bg_manager.kill(pid, delete_log)
    }
}
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use async_trait::async_trait;
    use forge_app::domain::{CommandOutput, Environment};
    use forge_app::{CommandInfra, ShellService};
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockCommandInfra {
        expected_env_vars: Option<Vec<String>>,
    }

    #[async_trait]
    impl CommandInfra for MockCommandInfra {
        async fn execute_command(
            &self,
            command: String,
            _working_dir: PathBuf,
            _silent: bool,
            env_vars: Option<Vec<String>>,
        ) -> anyhow::Result<CommandOutput> {
            // Verify that environment variables are passed through correctly
            assert_eq!(env_vars, self.expected_env_vars);

            Ok(CommandOutput {
                stdout: "Mock output".to_string(),
                stderr: "".to_string(),
                command,
                exit_code: Some(0),
            })
        }

        async fn execute_command_raw(
            &self,
            _command: &str,
            _working_dir: PathBuf,
            _env_vars: Option<Vec<String>>,
        ) -> anyhow::Result<std::process::ExitStatus> {
            unimplemented!()
        }

        async fn execute_command_background(
            &self,
            command: String,
            _working_dir: PathBuf,
            _env_vars: Option<Vec<String>>,
        ) -> anyhow::Result<forge_domain::BackgroundCommandOutput> {
            let log_file = tempfile::Builder::new()
                .prefix("forge-bg-test-")
                .suffix(".log")
                .tempfile()
                .unwrap();
            let log_path = log_file.path().to_path_buf();
            Ok(forge_domain::BackgroundCommandOutput {
                command,
                pid: 9999,
                log_file: log_path,
                log_handle: log_file,
            })
        }
    }

    impl EnvironmentInfra for MockCommandInfra {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            Some("mock_value".to_string())
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        fn is_restricted(&self) -> bool {
            false
        }
    }

    fn make_shell(expected_env_vars: Option<Vec<String>>) -> ForgeShell<MockCommandInfra> {
        ForgeShell::new(Arc::new(MockCommandInfra { expected_env_vars }))
    }

    /// Extracts the foreground CommandOutput from a ShellOutput, panicking if
    /// the variant is Background.
    fn unwrap_foreground(output: &ShellOutput) -> &forge_domain::CommandOutput {
        output.foreground().expect("Expected Foreground variant")
    }

    #[tokio::test]
    async fn test_shell_service_forwards_env_vars() {
        let fixture = make_shell(Some(vec!["PATH".to_string(), "HOME".to_string()]));

        let actual = fixture
            .execute(
                "echo hello".to_string(),
                PathBuf::from("."),
                false,
                false,
                false,
                Some(vec!["PATH".to_string(), "HOME".to_string()]),
                None,
            )
            .await
            .unwrap();

        let fg = unwrap_foreground(&actual);
        assert_eq!(fg.stdout, "Mock output");
        assert_eq!(fg.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_shell_service_forwards_no_env_vars() {
        let fixture = make_shell(None);

        let actual = fixture
            .execute(
                "echo hello".to_string(),
                PathBuf::from("."),
                false,
                false,
                false,
                None,
                None,
            )
            .await
            .unwrap();

        let fg = unwrap_foreground(&actual);
        assert_eq!(fg.stdout, "Mock output");
        assert_eq!(fg.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_shell_service_forwards_empty_env_vars() {
        let fixture = make_shell(Some(vec![]));

        let actual = fixture
            .execute(
                "echo hello".to_string(),
                PathBuf::from("."),
                false,
                false,
                false,
                Some(vec![]),
                None,
            )
            .await
            .unwrap();

        let fg = unwrap_foreground(&actual);
        assert_eq!(fg.stdout, "Mock output");
        assert_eq!(fg.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_shell_service_with_description() {
        let fixture = make_shell(None);

        let actual = fixture
            .execute(
                "echo hello".to_string(),
                PathBuf::from("."),
                false,
                false,
                false,
                None,
                Some("Prints hello to stdout".to_string()),
            )
            .await
            .unwrap();

        match &actual.kind {
            ShellOutputKind::Foreground(output) => {
                assert_eq!(output.stdout, "Mock output");
                assert_eq!(output.exit_code, Some(0));
            }
            _ => panic!("Expected Foreground"),
        }
        assert_eq!(
            actual.description,
            Some("Prints hello to stdout".to_string())
        );
    }

    #[tokio::test]
    async fn test_shell_service_without_description() {
        let fixture = make_shell(None);

        let actual = fixture
            .execute(
                "echo hello".to_string(),
                PathBuf::from("."),
                false,
                false,
                false,
                None,
                None,
            )
            .await
            .unwrap();

        match &actual.kind {
            ShellOutputKind::Foreground(output) => {
                assert_eq!(output.stdout, "Mock output");
                assert_eq!(output.exit_code, Some(0));
            }
            _ => panic!("Expected Foreground"),
        }
        assert_eq!(actual.description, None);
    }

    #[tokio::test]
    async fn test_shell_service_background_execution() {
        let fixture = make_shell(None);

        let actual = fixture
            .execute(
                "npm start".to_string(),
                PathBuf::from("."),
                false,
                false,
                true,
                None,
                Some("Start dev server".to_string()),
            )
            .await
            .unwrap();

        match &actual.kind {
            ShellOutputKind::Background { pid, .. } => {
                assert_eq!(*pid, 9999);
            }
            _ => panic!("Expected Background"),
        }

        let tracked = fixture.list_background_processes().unwrap();
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked[0].0.pid, 9999);
    }
}
