use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use forge_domain::HookExecutionResult;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

/// Default timeout for hook commands (10 minutes).
const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_secs(600);

/// Executes user hook shell commands with stdin piping and timeout support.
///
/// Uses `tokio::process::Command` directly (not `CommandInfra`) because we
/// need stdin piping which the existing infrastructure doesn't support.
pub struct UserHookExecutor;

impl UserHookExecutor {
    /// Executes a shell command, piping `input_json` to stdin and capturing
    /// stdout/stderr.
    ///
    /// # Arguments
    /// * `command` - The shell command string to execute.
    /// * `input_json` - JSON string to pipe to the command's stdin.
    /// * `timeout` - Optional per-hook timeout in milliseconds. Falls back to
    ///   `default_timeout_ms` when `None`.
    /// * `default_timeout_ms` - Default timeout in milliseconds from the
    ///   environment configuration. Uses the built-in default (10 min) when
    ///   zero.
    /// * `cwd` - Working directory for the command.
    /// * `env_vars` - Additional environment variables to set.
    ///
    /// # Errors
    /// Returns an error if the process cannot be spawned.
    pub async fn execute(
        command: &str,
        input_json: &str,
        timeout: Option<u64>,
        default_timeout_ms: u64,
        cwd: &PathBuf,
        env_vars: &HashMap<String, String>,
    ) -> anyhow::Result<HookExecutionResult> {
        let timeout_duration = timeout
            .map(Duration::from_millis)
            .unwrap_or_else(|| {
                if default_timeout_ms > 0 {
                    Duration::from_millis(default_timeout_ms)
                } else {
                    DEFAULT_HOOK_TIMEOUT
                }
            });

        debug!(
            command = command,
            cwd = %cwd.display(),
            timeout_ms = timeout_duration.as_millis() as u64,
            "Executing user hook command"
        );

        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };
        let shell_arg = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        let mut child = tokio::process::Command::new(shell)
            .arg(shell_arg)
            .arg(command)
            .current_dir(cwd)
            .envs(env_vars)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Pipe JSON input to stdin
        if let Some(mut stdin) = child.stdin.take() {
            let input = input_json.to_string();
            tokio::spawn(async move {
                let _ = stdin.write_all(input.as_bytes()).await;
                let _ = stdin.shutdown().await;
            });
        }

        // Wait for the command with timeout.
        // Note: `wait_with_output()` takes ownership of `child`. On timeout,
        // the future is dropped, and tokio will clean up the child process.
        let result = tokio::time::timeout(timeout_duration, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                debug!(
                    command = command,
                    exit_code = ?exit_code,
                    stdout_len = stdout.len(),
                    stderr_len = stderr.len(),
                    "Hook command completed"
                );

                Ok(HookExecutionResult { exit_code, stdout, stderr })
            }
            Ok(Err(e)) => {
                warn!(command = command, error = %e, "Hook command failed to execute");
                Err(e.into())
            }
            Err(_) => {
                warn!(
                    command = command,
                    timeout_ms = timeout_duration.as_millis() as u64,
                    "Hook command timed out"
                );
                // Process is already consumed by wait_with_output, tokio
                // handles cleanup when the future is dropped.
                Ok(HookExecutionResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!(
                        "Hook command timed out after {}ms",
                        timeout_duration.as_millis()
                    ),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn test_execute_simple_command() {
        let cwd = std::env::current_dir().unwrap();
        let actual =
            UserHookExecutor::execute("echo hello", "{}", None, 0, &cwd, &HashMap::new())
                .await
                .unwrap();

        assert_eq!(actual.exit_code, Some(0));
        assert_eq!(actual.stdout.trim(), "hello");
        assert!(actual.is_success());
    }

    #[tokio::test]
    async fn test_execute_reads_stdin() {
        let cwd = std::env::current_dir().unwrap();
        let actual = UserHookExecutor::execute(
            "cat",
            r#"{"hook_event_name": "PreToolUse"}"#,
            None,
            0,
            &cwd,
            &HashMap::new(),
        )
        .await
        .unwrap();

        assert_eq!(actual.exit_code, Some(0));
        assert!(actual.stdout.contains("PreToolUse"));
    }

    #[tokio::test]
    async fn test_execute_exit_code_2() {
        let cwd = std::env::current_dir().unwrap();
        let actual = UserHookExecutor::execute(
            "echo 'blocked' >&2; exit 2",
            "{}",
            None,
            0,
            &cwd,
            &HashMap::new(),
        )
        .await
        .unwrap();

        assert_eq!(actual.exit_code, Some(2));
        assert!(actual.is_blocking_exit());
        assert!(actual.stderr.contains("blocked"));
    }

    #[tokio::test]
    async fn test_execute_non_blocking_error() {
        let cwd = std::env::current_dir().unwrap();
        let actual = UserHookExecutor::execute("exit 1", "{}", None, 0, &cwd, &HashMap::new())
            .await
            .unwrap();

        assert_eq!(actual.exit_code, Some(1));
        assert!(actual.is_non_blocking_error());
    }

    #[tokio::test]
    async fn test_execute_timeout() {
        let cwd = std::env::current_dir().unwrap();
        let actual = UserHookExecutor::execute(
            "sleep 10",
            "{}",
            Some(100), // 100ms timeout
            0,
            &cwd,
            &HashMap::new(),
        )
        .await
        .unwrap();

        // Should have no exit code (killed by timeout)
        assert!(actual.exit_code.is_none() || actual.is_non_blocking_error());
        assert!(actual.stderr.contains("timed out"));
    }

    #[tokio::test]
    async fn test_execute_with_env_vars() {
        let cwd = std::env::current_dir().unwrap();
        let mut env_vars = HashMap::new();
        env_vars.insert("FORGE_TEST_VAR".to_string(), "test_value".to_string());

        let actual =
            UserHookExecutor::execute("echo $FORGE_TEST_VAR", "{}", None, 0, &cwd, &env_vars)
                .await
                .unwrap();

        assert_eq!(actual.exit_code, Some(0));
        assert_eq!(actual.stdout.trim(), "test_value");
    }

    #[tokio::test]
    async fn test_execute_json_output() {
        let cwd = std::env::current_dir().unwrap();
        let actual = UserHookExecutor::execute(
            r#"echo '{"decision":"block","reason":"test"}'"#,
            "{}",
            None,
            0,
            &cwd,
            &HashMap::new(),
        )
        .await
        .unwrap();

        assert!(actual.is_success());
        let output = actual.parse_output().unwrap();
        assert!(output.is_blocking());
        assert_eq!(output.reason, Some("test".to_string()));
    }
}
