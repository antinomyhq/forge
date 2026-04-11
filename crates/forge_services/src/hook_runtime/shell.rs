//! Shell hook executor — runs a `ShellHookCommand` as a subprocess.
//!
//! Implements the wire protocol described in
//! `claude-code/src/utils/hooks.ts:747-1335`:
//!
//! 1. Serialize the [`HookInput`] to JSON (snake_case fields matching the
//!    Claude Code wire format exactly).
//! 2. Spawn `bash -c <command>` (or `powershell -Command <command>` on Windows,
//!    if the config requests it).
//! 3. Write the JSON + a trailing `\n` to stdin. The newline is **critical** —
//!    shell hooks that use `read -r` patterns rely on it to complete their read
//!    loop.
//! 4. Close stdin immediately so the hook can exit without a partial read.
//! 5. Wait for the child with a timeout. Default timeout is 30 seconds to match
//!    Claude Code's `TOOL_HOOK_EXECUTION_TIMEOUT_MS`.
//! 6. Attempt to parse stdout as a [`HookOutput`] JSON document; fall back to
//!    treating the output as plain text when parsing fails.
//! 7. Classify the outcome using the JSON `decision` field when present,
//!    otherwise the raw exit code.
//!
//! The executor is stateless.  Basic `async` (fire-and-forget)
//! and `asyncRewake` (background-collect + observability logging) are
//! handled directly in [`ForgeShellHookExecutor::execute`].

use std::collections::HashMap;
use std::time::Duration;

use forge_app::{HookExecResult, HookOutcome};
use forge_domain::{
    HookDecision, HookInput, HookOutput, HookPromptRequest, HookPromptResponse, PendingHookResult,
    ShellHookCommand, ShellType, SyncHookOutput,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

/// Default timeout when a hook doesn't set its own.
///
/// Matches Claude Code's `TOOL_HOOK_EXECUTION_TIMEOUT_MS = 30000`.
const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// Abstraction for handling interactive prompt requests from hooks.
///
/// This is implemented by the top-level executor which has access to the
/// [`forge_app::HookExecutorInfra`] trait. The shell executor calls
/// through this trait when it detects a prompt request JSON line in
/// the hook's stdout.
#[async_trait::async_trait]
pub trait PromptHandler {
    async fn handle_prompt(&self, request: HookPromptRequest)
    -> anyhow::Result<HookPromptResponse>;
}

/// Errors that can occur during the streaming stdout read.
enum StreamingError {
    /// The hook timed out.
    Timeout,
    /// The child process wait failed.
    Wait(std::io::Error),
}

/// Executes [`ShellHookCommand`] hooks.
///
/// Shell-based hook executor. HTTP, prompt, and agent support are
/// provided by other implementations behind the same
/// [`forge_app::HookExecutorInfra`] trait.
#[derive(Debug, Clone)]
pub struct ForgeShellHookExecutor {
    default_timeout: Duration,
    /// Optional sender for async-rewake hook results. When set, the
    /// background `tokio::spawn` task sends [`PendingHookResult`] values
    /// through this channel instead of merely logging them.
    async_result_tx: Option<tokio::sync::mpsc::UnboundedSender<PendingHookResult>>,
}

impl Default for ForgeShellHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeShellHookExecutor {
    /// Create a new shell executor using the default 30-second timeout.
    pub fn new() -> Self {
        Self { default_timeout: DEFAULT_HOOK_TIMEOUT, async_result_tx: None }
    }

    /// Create a shell executor with a custom default timeout (used in
    /// tests to avoid sleeping for 30 s on the timeout path).
    #[cfg(test)]
    pub fn with_default_timeout(default_timeout: Duration) -> Self {
        Self { default_timeout, async_result_tx: None }
    }

    /// Attach an unbounded sender for async-rewake results.
    ///
    /// When set, the background `tokio::spawn` task for `asyncRewake`
    /// hooks will send [`PendingHookResult`] values through this
    /// channel. The receiver side is expected to push them into the
    /// [`AsyncHookResultQueue`](forge_app::AsyncHookResultQueue).
    pub fn with_async_result_tx(
        mut self,
        tx: tokio::sync::mpsc::UnboundedSender<PendingHookResult>,
    ) -> Self {
        self.async_result_tx = Some(tx);
        self
    }

    /// Run `config` with `input` piped to stdin.
    ///
    /// `env_vars` are layered on top of the inherited parent environment.
    /// Variable substitution (`${FORGE_PLUGIN_ROOT}` etc.) is applied to
    /// `config.command` before spawning.
    ///
    /// When `prompt_handler` is `Some`, the executor uses a streaming
    /// stdout reader that detects prompt request JSON lines and
    /// handles them bidirectionally (writing responses back to stdin).
    /// When `None`, prompt requests are detected but only logged as
    /// warnings (existing behavior).
    pub async fn execute(
        &self,
        config: &ShellHookCommand,
        input: &HookInput,
        env_vars: HashMap<String, String>,
        prompt_handler: Option<&(dyn PromptHandler + Send + Sync)>,
    ) -> anyhow::Result<HookExecResult> {
        // 1. Serialize the input.
        let input_json = serde_json::to_string(input)?;

        // 2. Substitute ${VAR} references in the command string.
        let command = substitute_variables(&config.command, &env_vars);

        // 3. Pick shell based on config (default bash on Unix, powershell on Windows is
        //    handled implicitly by the fallback on Windows builds; defaults to bash
        //    everywhere because the test suite is gated to unix).
        let (program, shell_flag) = match config.shell {
            Some(ShellType::Powershell) => ("powershell", "-Command"),
            Some(ShellType::Bash) | None => ("bash", "-c"),
        };

        let mut cmd = Command::new(program);
        cmd.arg(shell_flag)
            .arg(&command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // For async (fire-and-forget) hooks the child must outlive this
        // function, so we must NOT set `kill_on_drop`. For normal hooks
        // we still want the child killed if the future is dropped (e.g.
        // on timeout).
        if !config.async_mode {
            cmd.kill_on_drop(true);
        }

        for (key, val) in &env_vars {
            cmd.env(key, val);
        }

        let mut child = cmd.spawn()?;

        // 4. Write JSON + "\n" to stdin. When prompt_handler is active we keep the
        //    stdin handle alive so we can write prompt responses later. Otherwise we
        //    drop it immediately so the hook sees EOF.
        let mut stdin_handle = child.stdin.take();
        if let Some(stdin) = &mut stdin_handle {
            // Write input to stdin; ignore BrokenPipe (EPIPE) errors that
            // occur when the hook closes stdin before we finish writing.
            // This matches Claude Code's EPIPE handling at hooks.ts:1288-1299.
            let write_result = async {
                stdin.write_all(input_json.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                Ok::<(), std::io::Error>(())
            }
            .await;

            if let Err(e) = write_result
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(anyhow::anyhow!("hook stdin write failed: {e}"));
            }
            // BrokenPipe is expected when the hook doesn't read stdin.
            // Continue to collect stdout/stderr and exit code normally.
        }
        // When there is no prompt handler, close stdin now so the hook
        // sees EOF (original behavior).
        if prompt_handler.is_none() {
            drop(stdin_handle.take());
        }

        // 5a. Async (fire-and-forget): return Success immediately and
        //     detach the child into a background task for cleanup.
        //     When `async_rewake` is true, the background task collects
        //     stdout/stderr and parses the result for observability
        //     (mirroring Claude Code's asyncRewake behaviour).
        if config.async_mode {
            // Async hooks don't use the prompt protocol — always close
            // stdin before detaching the child.
            drop(stdin_handle.take());
            let async_rewake = config.async_rewake;
            let hook_name = config.command.clone();
            let result_tx = self.async_result_tx.clone();
            tokio::spawn(async move {
                match child.wait_with_output().await {
                    Ok(output) => {
                        let exit_code = output.status.code();
                        if async_rewake {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            // Parse stdout as HookOutput for logging.
                            let parsed = if stdout.trim_start().starts_with('{') {
                                serde_json::from_str::<HookOutput>(&stdout).ok()
                            } else {
                                None
                            };
                            tracing::info!(
                                exit_code = ?exit_code,
                                has_output = parsed.is_some(),
                                stderr = %stderr.trim(),
                                "asyncRewake hook completed"
                            );

                            // Send the result through the channel if a
                            // sender is available. exit_code 2 is
                            // blocking; exit_code 0 is success.
                            if let Some(tx) = result_tx {
                                let is_blocking = exit_code == Some(2);
                                let message = if is_blocking {
                                    // For blocking, prefer stderr; fall
                                    // back to stdout.
                                    let s = stderr.trim();
                                    if s.is_empty() {
                                        stdout.trim().to_string()
                                    } else {
                                        s.to_string()
                                    }
                                } else {
                                    // For success, use parsed output
                                    // message or raw stdout.
                                    parsed
                                        .as_ref()
                                        .and_then(|o| match o {
                                            HookOutput::Sync(sync) => sync.system_message.clone(),
                                            _ => None,
                                        })
                                        .unwrap_or_else(|| stdout.trim().to_string())
                                };
                                if !message.is_empty() {
                                    let _ = tx.send(PendingHookResult {
                                        hook_name: hook_name.clone(),
                                        message,
                                        is_blocking,
                                    });
                                }
                            }
                        } else {
                            tracing::debug!(exit_code = ?exit_code, "async hook completed");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "async hook wait failed");
                    }
                }
            });
            return Ok(HookExecResult {
                outcome: HookOutcome::Success,
                output: None,
                raw_stdout: String::new(),
                raw_stderr: String::new(),
                exit_code: None,
            });
        }

        // 5b. Wait with timeout (synchronous hooks).
        let timeout_duration = config
            .timeout
            .map(Duration::from_secs)
            .unwrap_or(self.default_timeout);

        // Use streaming stdout reader so we can detect and handle prompt
        // requests bidirectionally. This replaces the old
        // `child.wait_with_output()` batch read.
        let streaming_result = self
            .execute_sync_streaming(&mut child, stdin_handle, timeout_duration, prompt_handler)
            .await;

        let (stdout, stderr, exit_code) = match streaming_result {
            Ok(result) => result,
            Err(StreamingError::Timeout) => {
                // Child is killed by `kill_on_drop` when we return here.
                return Ok(HookExecResult {
                    outcome: HookOutcome::Cancelled,
                    output: None,
                    raw_stdout: String::new(),
                    raw_stderr: format!("hook timed out after {}s", timeout_duration.as_secs()),
                    exit_code: None,
                });
            }
            Err(StreamingError::Wait(e)) => {
                return Err(anyhow::anyhow!("hook wait failed: {e}"));
            }
        };

        // 6. Try to parse stdout as a HookOutput JSON document.
        let parsed_output = if stdout.trim_start().starts_with('{') {
            serde_json::from_str::<HookOutput>(&stdout).ok()
        } else {
            None
        };

        // 7. Classify the outcome.
        let outcome = classify_outcome(exit_code, parsed_output.as_ref());

        Ok(HookExecResult {
            outcome,
            output: parsed_output,
            raw_stdout: stdout,
            raw_stderr: stderr,
            exit_code,
        })
    }

    /// Streaming stdout reader for synchronous hooks.
    ///
    /// Reads stdout line-by-line via [`BufReader`], detecting prompt
    /// request JSON lines and handling them bidirectionally when a
    /// `prompt_handler` is provided. Stderr is collected in a
    /// background task. The entire operation is wrapped in a timeout.
    ///
    /// Returns `(stdout_string, stderr_string, exit_code)` on success.
    async fn execute_sync_streaming(
        &self,
        child: &mut tokio::process::Child,
        mut stdin_handle: Option<tokio::process::ChildStdin>,
        timeout_duration: Duration,
        prompt_handler: Option<&(dyn PromptHandler + Send + Sync)>,
    ) -> Result<(String, String, Option<i32>), StreamingError> {
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        // Collect stderr in a background task so it doesn't block stdout
        // processing.
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stderr) = stderr_pipe {
                tokio::io::AsyncReadExt::read_to_end(&mut stderr, &mut buf)
                    .await
                    .ok();
            }
            buf
        });

        let mut stdout_buf = Vec::<u8>::new();

        // The inner future reads stdout line-by-line and handles prompt
        // requests when detected. It is wrapped in a `tokio::time::timeout`
        // below.
        let inner = async {
            if let Some(stdout_pipe) = stdout_pipe {
                let reader = BufReader::new(stdout_pipe);
                let mut lines = reader.lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    // Quick heuristic: does this look like a prompt request?
                    if line.trim_start().starts_with('{')
                        && line.contains("\"prompt\"")
                        && let Ok(req) = serde_json::from_str::<HookPromptRequest>(&line)
                    {
                        // We have a valid prompt request.
                        if let Some(handler) = prompt_handler {
                            match handler.handle_prompt(req).await {
                                Ok(response) => {
                                    if let Some(stdin) = &mut stdin_handle {
                                        let resp_json =
                                            serde_json::to_string(&response).unwrap_or_default();
                                        let write_result = async {
                                            stdin.write_all(resp_json.as_bytes()).await?;
                                            stdin.write_all(b"\n").await?;
                                            stdin.flush().await?;
                                            Ok::<(), std::io::Error>(())
                                        }
                                        .await;
                                        if let Err(e) = write_result
                                            && e.kind() != std::io::ErrorKind::BrokenPipe
                                        {
                                            tracing::warn!(
                                                error = %e,
                                                "Failed to write prompt response to hook stdin"
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        "Hook prompt handler returned error, closing stdin"
                                    );
                                    // Drop stdin so the hook gets EOF and
                                    // can exit gracefully.
                                    stdin_handle = None;
                                }
                            }
                        } else {
                            // No prompt handler — log a warning for
                            // observability (matches the old
                            // `detect_prompt_request` behavior).
                            let message = &req.prompt.message;
                            tracing::warn!(
                                message = %message,
                                "Hook requested interactive prompt but no prompt \
                                 handler is available — the hook may time out."
                            );
                            // Drop stdin so the hook gets EOF instead of
                            // blocking forever.
                            stdin_handle = None;
                        }
                        // Prompt request lines are stripped from the
                        // final stdout (matching CC's
                        // `processedPromptLines` behavior).
                        continue;
                    }
                    // Regular stdout line — accumulate into buffer.
                    stdout_buf.extend_from_slice(line.as_bytes());
                    stdout_buf.push(b'\n');
                }
            }
        };

        // Wrap the streaming loop in a timeout.
        if timeout(timeout_duration, inner).await.is_err() {
            // Timeout — kill child.
            child.kill().await.ok();
            return Err(StreamingError::Timeout);
        }

        // Drop stdin so the hook sees EOF if it's still reading.
        drop(stdin_handle);

        // Wait for the child to exit.
        let status = child.wait().await.map_err(StreamingError::Wait)?;
        let stderr_buf = stderr_task.await.unwrap_or_default();

        let stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
        let stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
        let exit_code = status.code();

        Ok((stdout, stderr, exit_code))
    }
}

/// Decide the [`HookOutcome`] using (in priority order):
///
/// 1. A parsed [`SyncHookOutput`]'s `decision` field, if `Block`.
/// 2. The raw exit code: `0` → `Success`, `2` → `Blocking`, other non-zero /
///    missing → `NonBlockingError`.
fn classify_outcome(exit_code: Option<i32>, output: Option<&HookOutput>) -> HookOutcome {
    if let Some(HookOutput::Sync(SyncHookOutput { decision: Some(dec), .. })) = output
        && matches!(dec, HookDecision::Block)
    {
        return HookOutcome::Blocking;
    }

    match exit_code {
        Some(0) => HookOutcome::Success,
        Some(2) => HookOutcome::Blocking,
        Some(_) => HookOutcome::NonBlockingError,
        None => HookOutcome::NonBlockingError,
    }
}

/// Substitute `${VAR}` and `${user_config.KEY}` references in a command
/// string using the given environment variable map.
///
/// Only `${VAR}` (braced) references are substituted here — the bare
/// `$VAR` form is left for the shell itself to expand.
///
/// `${user_config.KEY}` is resolved by looking up
/// `FORGE_PLUGIN_OPTION_<KEY>` in `env_vars` (key is upper-cased, hyphens
/// become underscores). This mirrors Claude Code's plugin user-config
/// substitution at `claude-code/src/utils/hooks.ts:822-857`.
///
/// Reference: `claude-code/src/utils/hooks.ts:822-857`.
pub fn substitute_variables(command: &str, env_vars: &HashMap<String, String>) -> String {
    let mut result = command.to_string();

    // Handle ${user_config.KEY} substitutions first so they don't collide
    // with the generic ${VAR} pass below.
    let prefix = "${user_config.";
    while let Some(start) = result.find(prefix) {
        if let Some(rel_end) = result[start..].find('}') {
            let key = &result[start + prefix.len()..start + rel_end];
            let env_key = format!(
                "FORGE_PLUGIN_OPTION_{}",
                key.to_uppercase().replace('-', "_")
            );
            let replacement = env_vars.get(&env_key).map(String::as_str).unwrap_or("");
            result = format!(
                "{}{}{}",
                &result[..start],
                replacement,
                &result[start + rel_end + 1..]
            );
        } else {
            break;
        }
    }

    // Handle regular ${VAR} substitutions.
    for (key, val) in env_vars {
        let braced = format!("${{{key}}}");
        if result.contains(&braced) {
            result = result.replace(&braced, val);
        }
    }
    result
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use forge_domain::{HookInputBase, HookInputPayload};
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn sample_input() -> HookInput {
        HookInput {
            base: HookInputBase {
                session_id: "sess-test".to_string(),
                transcript_path: PathBuf::from("/tmp/transcript.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "PreToolUse".to_string(),
            },
            payload: HookInputPayload::PreToolUse {
                tool_name: "Bash".to_string(),
                tool_input: json!({"command": "ls"}),
                tool_use_id: "toolu_test".to_string(),
            },
        }
    }

    fn shell_hook(command: &str) -> ShellHookCommand {
        ShellHookCommand {
            command: command.to_string(),
            condition: None,
            shell: Some(ShellType::Bash),
            timeout: None,
            status_message: None,
            once: false,
            async_mode: false,
            async_rewake: false,
        }
    }

    #[tokio::test]
    async fn test_hook_with_json_stdout_parses_to_hook_output() {
        let executor = ForgeShellHookExecutor::new();
        let config = shell_hook(r#"echo '{"continue": true, "systemMessage": "from hook"}'"#);
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
        assert!(matches!(result.output, Some(HookOutput::Sync(_))));
        match result.output {
            Some(HookOutput::Sync(sync)) => {
                assert_eq!(sync.should_continue, Some(true));
                assert_eq!(sync.system_message.as_deref(), Some("from hook"));
            }
            other => panic!("expected Sync output, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_hook_with_plain_text_stdout_is_success() {
        let executor = ForgeShellHookExecutor::new();
        let config = shell_hook("echo hello world");
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.output.is_none());
        assert_eq!(result.raw_stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_hook_exit_code_2_is_blocking() {
        let executor = ForgeShellHookExecutor::new();
        let config = shell_hook("echo 'nope' 1>&2; exit 2");
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Blocking);
        assert_eq!(result.exit_code, Some(2));
        assert_eq!(result.raw_stderr.trim(), "nope");
    }

    #[tokio::test]
    async fn test_hook_exit_code_1_is_non_blocking_error() {
        let executor = ForgeShellHookExecutor::new();
        let config = shell_hook("exit 1");
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::NonBlockingError);
        assert_eq!(result.exit_code, Some(1));
    }

    #[tokio::test]
    async fn test_hook_stdin_receives_exact_snake_case_json() {
        let temp = TempDir::new().unwrap();
        let captured = temp.path().join("captured.json");
        let executor = ForgeShellHookExecutor::new();

        // The hook writes its stdin contents to a file so the test can
        // inspect them.
        let command = format!("cat > {}", captured.display());
        let config = shell_hook(&command);
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);

        let contents = std::fs::read_to_string(&captured).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();

        assert_eq!(parsed["session_id"], "sess-test");
        assert_eq!(parsed["hook_event_name"], "PreToolUse");
        assert_eq!(parsed["tool_name"], "Bash");
        assert_eq!(parsed["tool_use_id"], "toolu_test");
        assert_eq!(parsed["tool_input"]["command"], "ls");
    }

    #[tokio::test]
    async fn test_hook_env_vars_are_set_in_subprocess() {
        let temp = TempDir::new().unwrap();
        let captured = temp.path().join("env.txt");
        let executor = ForgeShellHookExecutor::new();

        let command = format!(
            "printf '%s|%s' \"$FORGE_PROJECT_DIR\" \"$FORGE_SESSION_ID\" > {}",
            captured.display()
        );
        let config = shell_hook(&command);

        let mut env = HashMap::new();
        env.insert("FORGE_PROJECT_DIR".to_string(), "/proj-test".to_string());
        env.insert("FORGE_SESSION_ID".to_string(), "sess-env".to_string());

        executor
            .execute(&config, &sample_input(), env, None)
            .await
            .unwrap();

        let captured_text = std::fs::read_to_string(&captured).unwrap();
        assert_eq!(captured_text, "/proj-test|sess-env");
    }

    #[tokio::test]
    async fn test_command_substitution_replaces_braced_variable() {
        let temp = TempDir::new().unwrap();
        let captured = temp.path().join("plugin-root.txt");
        let executor = ForgeShellHookExecutor::new();

        // The literal ${FORGE_PLUGIN_ROOT} is substituted by us (not the
        // shell) before spawning.
        let command = format!("echo '${{FORGE_PLUGIN_ROOT}}' > {}", captured.display());
        let config = shell_hook(&command);

        let mut env = HashMap::new();
        env.insert("FORGE_PLUGIN_ROOT".to_string(), "/plugins/demo".to_string());

        executor
            .execute(&config, &sample_input(), env, None)
            .await
            .unwrap();

        let contents = std::fs::read_to_string(&captured).unwrap();
        assert_eq!(contents.trim(), "/plugins/demo");
    }

    #[tokio::test]
    async fn test_hook_timeout_produces_cancelled() {
        // Use a very short timeout and a long-running hook.
        let executor = ForgeShellHookExecutor::with_default_timeout(Duration::from_millis(100));
        let config = shell_hook("sleep 5");
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Cancelled);
        assert!(result.exit_code.is_none());
        assert!(result.raw_stderr.contains("timed out"));
    }

    #[test]
    fn test_substitute_variables_replaces_braced_references() {
        let mut env = HashMap::new();
        env.insert("FORGE_PLUGIN_ROOT".to_string(), "/plugins/x".to_string());
        env.insert("FORGE_SESSION_ID".to_string(), "sess-1".to_string());

        let actual = substitute_variables(
            "run ${FORGE_PLUGIN_ROOT}/bin --session ${FORGE_SESSION_ID}",
            &env,
        );
        assert_eq!(actual, "run /plugins/x/bin --session sess-1");
    }

    #[test]
    fn test_substitute_variables_leaves_unknown_vars_alone() {
        let env = HashMap::new();
        let actual = substitute_variables("echo ${UNKNOWN}", &env);
        assert_eq!(actual, "echo ${UNKNOWN}");
    }

    #[test]
    fn test_classify_outcome_json_block_overrides_exit_zero() {
        let output = HookOutput::Sync(SyncHookOutput {
            decision: Some(HookDecision::Block),
            ..Default::default()
        });
        let outcome = classify_outcome(Some(0), Some(&output));
        assert_eq!(outcome, HookOutcome::Blocking);
    }

    #[test]
    fn test_classify_outcome_exit_0_no_json_is_success() {
        assert_eq!(classify_outcome(Some(0), None), HookOutcome::Success);
    }

    #[test]
    fn test_classify_outcome_exit_2_no_json_is_blocking() {
        assert_eq!(classify_outcome(Some(2), None), HookOutcome::Blocking);
    }

    #[test]
    fn test_classify_outcome_exit_1_no_json_is_non_blocking_error() {
        assert_eq!(
            classify_outcome(Some(1), None),
            HookOutcome::NonBlockingError
        );
    }

    #[tokio::test]
    async fn test_hook_that_ignores_stdin_does_not_panic() {
        // `true` immediately exits without reading stdin.
        // Previously this caused a BrokenPipe error.
        let executor = ForgeShellHookExecutor::new();
        let config = shell_hook("true");
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_async_hook_returns_immediately() {
        // An async hook should return Success almost instantly without
        // waiting for the child to finish. We use `sleep 10` as the
        // command — if we blocked, the test would take 10 s.
        let executor = ForgeShellHookExecutor::new();
        let config = ShellHookCommand { async_mode: true, ..shell_hook("sleep 10") };

        let start = std::time::Instant::now();
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, HookOutcome::Success);
        // No exit code — we didn't wait for the child.
        assert!(result.exit_code.is_none());
        assert!(result.raw_stdout.is_empty());
        assert!(result.raw_stderr.is_empty());
        assert!(result.output.is_none());
        // Must return in well under 2 seconds.
        assert!(
            elapsed < Duration::from_secs(2),
            "async hook took too long: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_async_hook_child_receives_stdin() {
        // Verify the async hook still receives stdin before we detach.
        let temp = TempDir::new().unwrap();
        let captured = temp.path().join("async_stdin.json");
        let executor = ForgeShellHookExecutor::new();

        let command = format!("cat > {}", captured.display());
        let config = ShellHookCommand { async_mode: true, ..shell_hook(&command) };
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);

        // Give the background child time to write and exit.
        // Use a retry loop rather than a fixed sleep to be more robust
        // across different CI environments.
        let mut contents = String::new();
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(c) = std::fs::read_to_string(&captured)
                && !c.trim().is_empty()
            {
                contents = c;
                break;
            }
        }

        assert!(!contents.is_empty(), "async hook child never wrote to file");
        let parsed: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(parsed["hook_event_name"], "PreToolUse");
    }

    #[tokio::test]
    async fn test_async_rewake_hook_logs_output() {
        // When `async_rewake` is true the background task should
        // collect stdout/stderr via `wait_with_output` and parse
        // the JSON output without panicking.  The execute call
        // itself must still return immediately (fire-and-forget).
        let executor = ForgeShellHookExecutor::new();
        let config = ShellHookCommand {
            async_mode: true,
            async_rewake: true,
            ..shell_hook(r#"echo '{"continue": true, "systemMessage": "rewake test"}'"#)
        };

        let start = std::time::Instant::now();
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();
        let elapsed = start.elapsed();

        // Must return instantly — no waiting for the child.
        assert_eq!(result.outcome, HookOutcome::Success);
        assert!(result.exit_code.is_none());
        assert!(result.raw_stdout.is_empty());
        assert!(result.raw_stderr.is_empty());
        assert!(result.output.is_none());
        assert!(
            elapsed < Duration::from_secs(2),
            "async_rewake hook took too long: {elapsed:?}"
        );

        // Give the background task enough time to finish and exercise the
        // parsing + logging path (this verifies no panic occurs).
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // ----------------------------------------------------------------
    // Prompt-request detection (streaming mode)
    // ----------------------------------------------------------------

    #[tokio::test]
    async fn test_prompt_request_stripped_from_stdout_no_handler() {
        // When no prompt handler is provided, prompt request lines are
        // stripped from stdout and the hook receives EOF on stdin so it
        // can exit. The non-prompt output should still be present.
        let executor = ForgeShellHookExecutor::new();
        // Hook writes a prompt request line, then a regular output line.
        // Because stdin is closed (no handler), `read` returns empty/EOF
        // and the hook continues to echo the final line.
        let config = shell_hook(
            r#"echo '{"prompt":{"type":"confirm","message":"Deploy?"}}'; echo '{"result":"done"}'"#,
        );
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
        // The prompt request line should be stripped from raw_stdout.
        assert!(!result.raw_stdout.contains("Deploy?"));
        // The regular output line should be present.
        assert!(result.raw_stdout.contains(r#"{"result":"done"}"#));
    }

    #[tokio::test]
    async fn test_hook_with_prompt_request_stdout_still_returns_result() {
        // A hook that emits a prompt request JSON to stdout should still
        // produce a normal HookExecResult (the prompt line is stripped).
        let executor = ForgeShellHookExecutor::new();
        let config = shell_hook(r#"echo '{"prompt": {"type": "confirm", "message": "Deploy?"}}'"#);
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        // The hook exits 0, so outcome is Success. The prompt line is
        // stripped from stdout.
        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
    }

    // ----------------------------------------------------------------
    // asyncRewake channel tests
    // ----------------------------------------------------------------

    #[tokio::test]
    async fn test_async_rewake_sends_blocking_to_channel() {
        // An asyncRewake hook that exits with code 2 should send a
        // PendingHookResult with is_blocking=true through the channel.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let executor = ForgeShellHookExecutor::new().with_async_result_tx(tx);
        let config = ShellHookCommand {
            async_mode: true,
            async_rewake: true,
            ..shell_hook("echo 'blocked' 1>&2; exit 2")
        };

        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        // The execute call returns immediately with Success.
        assert_eq!(result.outcome, HookOutcome::Success);
        assert!(result.exit_code.is_none());

        // Wait for the background task to complete and send.
        let pending = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out waiting for PendingHookResult")
            .expect("channel closed without sending");
        assert!(pending.is_blocking);
        assert_eq!(pending.message, "blocked");
        assert!(!pending.hook_name.is_empty());
    }

    #[tokio::test]
    async fn test_async_rewake_sends_success_to_channel() {
        // An asyncRewake hook that exits with code 0 and has stdout
        // should send a PendingHookResult with is_blocking=false.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let executor = ForgeShellHookExecutor::new().with_async_result_tx(tx);
        let config = ShellHookCommand {
            async_mode: true,
            async_rewake: true,
            ..shell_hook("echo 'done successfully'")
        };

        executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        let pending = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out waiting for PendingHookResult")
            .expect("channel closed without sending");
        assert!(!pending.is_blocking);
        assert_eq!(pending.message, "done successfully");
    }

    #[tokio::test]
    async fn test_async_no_rewake_does_not_send_to_channel() {
        // A plain async hook (async_rewake=false) should NOT send
        // anything through the channel even when a sender is attached.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let executor = ForgeShellHookExecutor::new().with_async_result_tx(tx);
        let config = ShellHookCommand {
            async_mode: true,
            async_rewake: false,
            ..shell_hook("echo 'fire and forget'")
        };

        executor
            .execute(&config, &sample_input(), HashMap::new(), None)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(
            rx.try_recv().is_err(),
            "plain async hook should not send to channel"
        );
    }

    // ----------------------------------------------------------------
    // Bidirectional prompt protocol tests
    // ----------------------------------------------------------------

    /// Mock prompt handler that responds with `"yes-to-<message>"` for
    /// every prompt request. Used by the bidirectional stdin tests.
    struct MockPromptHandler;

    #[async_trait::async_trait]
    impl PromptHandler for MockPromptHandler {
        async fn handle_prompt(
            &self,
            request: forge_domain::HookPromptRequest,
        ) -> anyhow::Result<forge_domain::HookPromptResponse> {
            Ok(forge_domain::HookPromptResponse {
                response: format!("yes-to-{}", request.prompt.message),
            })
        }
    }

    #[tokio::test]
    async fn test_prompt_response_written_to_stdin() {
        // Hook script:
        // 1. Reads initial input JSON from stdin (line 1)
        // 2. Writes a prompt request to stdout
        // 3. Reads the prompt response from stdin (line 2)
        // 4. Writes final output including the response it received
        //
        // This verifies the full bidirectional protocol.
        let executor = ForgeShellHookExecutor::new();
        let handler = MockPromptHandler;
        let config = shell_hook(
            r#"read -r INPUT; echo '{"prompt":{"type":"confirm","message":"Deploy?"}}'; read -r RESPONSE; echo "got:$RESPONSE""#,
        );
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), Some(&handler))
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
        // The hook should have received our response and echoed it back.
        assert!(
            result.raw_stdout.contains("yes-to-Deploy?"),
            "expected response in stdout, got: {}",
            result.raw_stdout
        );
        // The prompt request line should NOT be in raw_stdout.
        assert!(
            !result.raw_stdout.contains(r#""prompt""#),
            "prompt request should be stripped from stdout"
        );
    }

    #[tokio::test]
    async fn test_multiple_prompt_requests_handled_sequentially() {
        // Hook issues two prompt requests, each time reading the response
        // from stdin before continuing.
        let executor = ForgeShellHookExecutor::new();
        let handler = MockPromptHandler;
        let config = shell_hook(
            r#"read -r INPUT; echo '{"prompt":{"type":"confirm","message":"First?"}}'; read -r R1; echo '{"prompt":{"type":"input","message":"Second?"}}'; read -r R2; echo "r1:$R1 r2:$R2""#,
        );
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), Some(&handler))
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
        assert!(
            result.raw_stdout.contains("yes-to-First?"),
            "expected first response, got: {}",
            result.raw_stdout
        );
        assert!(
            result.raw_stdout.contains("yes-to-Second?"),
            "expected second response, got: {}",
            result.raw_stdout
        );
    }

    #[tokio::test]
    async fn test_hook_exit_before_prompt_response_does_not_hang() {
        // Hook writes a prompt request but exits immediately without
        // waiting for a response. The executor must not hang.
        let executor = ForgeShellHookExecutor::with_default_timeout(Duration::from_secs(10));
        let handler = MockPromptHandler;
        // The hook writes a prompt request then exits immediately
        // (doesn't read stdin for the response).
        let config =
            shell_hook(r#"echo '{"prompt":{"type":"confirm","message":"Ignored?"}}'; echo "done""#);
        let start = std::time::Instant::now();
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), Some(&handler))
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.raw_stdout.contains("done"));
        // Must complete well before the timeout — under CI load the
        // process spawn + teardown may take a few seconds, so we allow
        // up to 8s (still safely below the 10s timeout).
        assert!(
            elapsed < Duration::from_secs(8),
            "hook exit should not cause hang: {elapsed:?}"
        );
    }

    /// Prompt handler that always returns an error (simulates headless mode).
    struct DenyPromptHandler;

    #[async_trait::async_trait]
    impl PromptHandler for DenyPromptHandler {
        async fn handle_prompt(
            &self,
            _request: forge_domain::HookPromptRequest,
        ) -> anyhow::Result<forge_domain::HookPromptResponse> {
            Err(anyhow::anyhow!("prompts not supported in headless mode"))
        }
    }

    #[tokio::test]
    async fn test_prompt_handler_error_closes_stdin() {
        // When the prompt handler returns Err, stdin should be closed
        // so the hook gets EOF and can exit gracefully.
        let executor = ForgeShellHookExecutor::new();
        let handler = DenyPromptHandler;
        // Hook writes prompt request, then tries to read response.
        // Since handler returns Err, stdin is closed, so `read` fails
        // and the hook exits.
        let config = shell_hook(
            r#"read -r INPUT; echo '{"prompt":{"type":"confirm","message":"Denied?"}}'; if read -r RESP; then echo "got:$RESP"; else echo "stdin-closed"; fi"#,
        );
        let result = executor
            .execute(&config, &sample_input(), HashMap::new(), Some(&handler))
            .await
            .unwrap();

        assert_eq!(result.outcome, HookOutcome::Success);
        assert_eq!(result.exit_code, Some(0));
        // The hook should detect that stdin was closed.
        assert!(
            result.raw_stdout.contains("stdin-closed"),
            "expected stdin-closed, got: {}",
            result.raw_stdout
        );
    }
}
