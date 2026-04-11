//! End-to-end hook execution tests for Wave G-2 (Phase 11.1.3).
//!
//! These tests exercise the real hook execution pipeline — config load →
//! match → execute → aggregate — using the Wave G-1 fixture plugins
//! checked in under `tests/fixtures/plugins/`.
//!
//! Because `forge_services::hook_runtime` is a private module, these
//! integration tests replicate the shell executor's wire protocol
//! directly via `tokio::process::Command` (spawn bash, pipe stdin JSON,
//! read stdout/stderr, classify by exit code). This mirrors exactly
//! what `ForgeShellHookExecutor::execute()` does at
//! `crates/forge_services/src/hook_runtime/shell.rs:73-158`.
//!
//! The matcher functions are accessed via their public re-export from
//! `forge_app::{matches_pattern, matches_condition}`.
//!
//! All tests are gated to `#[cfg(unix)]` because the shell hooks use
//! `bash -c <command>`.

#[cfg(unix)]
mod common;

#[cfg(unix)]
mod e2e {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use forge_app::hook_runtime::{HookConfigSource, HookMatcherWithSource, MergedHooksConfig};
    use forge_app::{matches_condition, matches_pattern};
    use forge_domain::{
        HookCommand, HookEventName, HookInput, HookInputBase, HookInputPayload, HookOutput,
        HooksConfig, ShellHookCommand,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use tokio::io::AsyncWriteExt;

    use crate::common::fixture_plugin_path;

    // ---------------------------------------------------------------
    // Shell execution helper (mirrors ForgeShellHookExecutor)
    // ---------------------------------------------------------------

    /// Result of executing a shell hook command.
    #[derive(Debug)]
    struct ShellExecResult {
        exit_code: Option<i32>,
        raw_stdout: String,
        raw_stderr: String,
        parsed_output: Option<HookOutput>,
    }

    impl ShellExecResult {
        fn is_success(&self) -> bool {
            self.exit_code == Some(0)
        }

        fn is_blocking(&self) -> bool {
            // Exit code 2 = blocking, or parsed JSON decision = Block.
            if let Some(HookOutput::Sync(ref sync)) = self.parsed_output
                && sync.decision == Some(forge_domain::HookDecision::Block)
            {
                return true;
            }
            self.exit_code == Some(2)
        }
    }

    /// Execute a shell hook command the same way `ForgeShellHookExecutor`
    /// does: serialize `HookInput` to JSON, pipe it to `bash -c <command>`
    /// on stdin, read stdout/stderr, and return the exit code + output.
    ///
    /// `env_vars` are substituted into `${VAR}` references in the command
    /// string before spawning, and also injected as real env vars.
    async fn execute_shell_hook(
        shell_cmd: &ShellHookCommand,
        input: &HookInput,
        env_vars: HashMap<String, String>,
    ) -> ShellExecResult {
        let input_json = serde_json::to_string(input).expect("HookInput serialization");

        // Substitute ${VAR} references in the command string.
        let command = substitute_variables(&shell_cmd.command, &env_vars);

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c")
            .arg(&command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        for (key, val) in &env_vars {
            cmd.env(key, val);
        }

        let mut child = cmd.spawn().expect("failed to spawn bash");

        // Write JSON + newline to stdin, then close.
        if let Some(mut stdin) = child.stdin.take() {
            // Ignore BrokenPipe — the hook may exit without reading stdin
            // (e.g. fire-and-forget loggers). Under concurrent execution the
            // child can finish before we write, closing the pipe.
            let _ = stdin.write_all(input_json.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
        }

        let output =
            tokio::time::timeout(std::time::Duration::from_secs(30), child.wait_with_output())
                .await
                .expect("hook timed out")
                .expect("hook wait failed");

        let raw_stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let raw_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let exit_code = output.status.code();

        let parsed_output = if raw_stdout.trim_start().starts_with('{') {
            serde_json::from_str::<HookOutput>(&raw_stdout).ok()
        } else {
            None
        };

        ShellExecResult { exit_code, raw_stdout, raw_stderr, parsed_output }
    }

    /// Substitute `${VAR}` references in a command string.
    /// Mirrors `shell.rs:190-199`.
    fn substitute_variables(command: &str, env_vars: &HashMap<String, String>) -> String {
        let mut result = command.to_string();
        for (key, val) in env_vars {
            let braced = format!("${{{key}}}");
            if result.contains(&braced) {
                result = result.replace(&braced, val);
            }
        }
        result
    }

    // ---------------------------------------------------------------
    // Fixture helpers
    // ---------------------------------------------------------------

    /// Read a fixture plugin's `hooks/hooks.json`, strip the outer
    /// `"hooks"` envelope, and parse the inner map as [`HooksConfig`].
    fn load_fixture_hooks_config(plugin_name: &str) -> HooksConfig {
        let path = fixture_plugin_path(plugin_name)
            .join("hooks")
            .join("hooks.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let envelope: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
        let inner = envelope
            .get("hooks")
            .unwrap_or_else(|| panic!("missing 'hooks' key in {}", path.display()));
        serde_json::from_value(inner.clone()).unwrap_or_else(|e| {
            panic!(
                "failed to parse inner HooksConfig from {}: {e}",
                path.display()
            )
        })
    }

    /// Extract the first [`ShellHookCommand`] for a given event.
    fn first_shell_command(config: &HooksConfig, event: &HookEventName) -> ShellHookCommand {
        let matchers = config
            .0
            .get(event)
            .unwrap_or_else(|| panic!("no matchers for event {event:?}"));
        match &matchers[0].hooks[0] {
            HookCommand::Command(shell) => shell.clone(),
            other => panic!("expected Command variant, got {other:?}"),
        }
    }

    /// Get the matcher pattern string for a given event.
    fn first_matcher_pattern(config: &HooksConfig, event: &HookEventName) -> String {
        let matchers = config
            .0
            .get(event)
            .unwrap_or_else(|| panic!("no matchers for event {event:?}"));
        matchers[0].matcher.clone().unwrap_or_default()
    }

    /// Construct a minimal `HookInput` for a `PreToolUse` event.
    fn pre_tool_use_input(tool_name: &str, tool_input: serde_json::Value) -> HookInput {
        HookInput {
            base: HookInputBase {
                session_id: "sess-e2e".to_string(),
                transcript_path: PathBuf::from("/tmp/transcript.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "PreToolUse".to_string(),
            },
            payload: HookInputPayload::PreToolUse {
                tool_name: tool_name.to_string(),
                tool_input,
                tool_use_id: "toolu_e2e_test".to_string(),
            },
        }
    }

    /// Construct a minimal `HookInput` for a `PostToolUse` event.
    fn post_tool_use_input(
        tool_name: &str,
        tool_input: serde_json::Value,
        tool_response: serde_json::Value,
    ) -> HookInput {
        HookInput {
            base: HookInputBase {
                session_id: "sess-e2e".to_string(),
                transcript_path: PathBuf::from("/tmp/transcript.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "PostToolUse".to_string(),
            },
            payload: HookInputPayload::PostToolUse {
                tool_name: tool_name.to_string(),
                tool_input,
                tool_response,
                tool_use_id: "toolu_e2e_test".to_string(),
            },
        }
    }

    // ===============================================================
    // a. test_shell_hook_receives_correct_stdin_json
    // ===============================================================

    #[tokio::test]
    async fn test_shell_hook_receives_correct_stdin_json() {
        // Use a temp-file capture command to verify the exact JSON
        // written to stdin.
        let temp = tempfile::TempDir::new().unwrap();
        let captured = temp.path().join("stdin.json");

        let capture_cmd = ShellHookCommand {
            command: format!("cat > {}", captured.display()),
            condition: None,
            shell: None,
            timeout: None,
            status_message: None,
            once: false,
            async_mode: false,
            async_rewake: false,
        };

        let input = pre_tool_use_input("Bash", json!({"command": "ls -la"}));

        let result = execute_shell_hook(&capture_cmd, &input, HashMap::new()).await;

        assert!(result.is_success(), "capture hook should exit 0");

        // Verify the captured stdin is valid JSON with expected fields.
        let raw = std::fs::read_to_string(&captured).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(raw.trim()).unwrap();
        assert_eq!(parsed["session_id"], "sess-e2e");
        assert_eq!(parsed["hook_event_name"], "PreToolUse");
        assert_eq!(parsed["tool_name"], "Bash");
        assert_eq!(parsed["tool_input"]["command"], "ls -la");
        assert_eq!(parsed["tool_use_id"], "toolu_e2e_test");
    }

    // ===============================================================
    // Bonus: dangerous-guard allows safe commands
    // ===============================================================

    #[tokio::test]
    async fn test_dangerous_guard_allows_safe_command() {
        let config = load_fixture_hooks_config("dangerous-guard");
        let shell_cmd = first_shell_command(&config, &HookEventName::PreToolUse);

        let input = pre_tool_use_input("Bash", json!({"command": "ls -la"}));

        let result = execute_shell_hook(&shell_cmd, &input, HashMap::new()).await;

        assert!(result.is_success());
        assert_eq!(result.exit_code, Some(0));
    }

    // ===============================================================
    // b. test_shell_hook_exit_2_blocks_tool_use
    // ===============================================================

    #[tokio::test]
    async fn test_shell_hook_exit_2_blocks_tool_use() {
        let config = load_fixture_hooks_config("dangerous-guard");
        let shell_cmd = first_shell_command(&config, &HookEventName::PreToolUse);

        // The hook checks for 'rm -rf /' in stdin and exits 2.
        let input = pre_tool_use_input("Bash", json!({"command": "rm -rf /"}));

        let result = execute_shell_hook(&shell_cmd, &input, HashMap::new()).await;

        assert!(result.is_blocking());
        assert_eq!(result.exit_code, Some(2));
        assert!(
            result.raw_stderr.contains("BLOCKED"),
            "stderr should contain 'BLOCKED', got: {:?}",
            result.raw_stderr,
        );
    }

    // ===============================================================
    // c. test_posttooluse_hook_returns_additional_context
    // ===============================================================

    #[tokio::test]
    async fn test_posttooluse_hook_returns_additional_context() {
        let config = load_fixture_hooks_config("prettier-format");
        let shell_cmd = first_shell_command(&config, &HookEventName::PostToolUse);

        let input = post_tool_use_input(
            "Write",
            json!({"file_path": "/tmp/test.ts"}),
            json!({"result": "ok"}),
        );

        let result = execute_shell_hook(&shell_cmd, &input, HashMap::new()).await;

        assert!(result.is_success());
        assert_eq!(result.exit_code, Some(0));

        // The hook echoes JSON with additional_context.
        let parsed: serde_json::Value = serde_json::from_str(result.raw_stdout.trim()).unwrap();
        assert_eq!(
            parsed["additional_context"], "Formatted file",
            "raw stdout JSON must contain additional_context field"
        );
    }

    // ===============================================================
    // d. test_matcher_filters_non_matching_tools
    // ===============================================================

    #[tokio::test]
    async fn test_matcher_filters_non_matching_tools() {
        // prettier-format's matcher is "Write|Edit" — should NOT match "Bash".
        let config = load_fixture_hooks_config("prettier-format");
        let pattern = first_matcher_pattern(&config, &HookEventName::PostToolUse);

        assert_eq!(pattern, "Write|Edit");
        assert!(
            matches_pattern(&pattern, "Write"),
            "matcher should match 'Write'"
        );
        assert!(
            matches_pattern(&pattern, "Edit"),
            "matcher should match 'Edit'"
        );
        assert!(
            !matches_pattern(&pattern, "Bash"),
            "matcher should NOT match 'Bash'"
        );
        assert!(
            !matches_pattern(&pattern, "Read"),
            "matcher should NOT match 'Read'"
        );
    }

    // ===============================================================
    // e. test_fire_and_forget_hook_writes_to_stderr
    // ===============================================================

    #[tokio::test]
    async fn test_fire_and_forget_hook_writes_to_stderr() {
        let config = load_fixture_hooks_config("bash-logger");
        let shell_cmd = first_shell_command(&config, &HookEventName::PreToolUse);

        let input = pre_tool_use_input("Bash", json!({"command": "echo hello"}));

        let result = execute_shell_hook(&shell_cmd, &input, HashMap::new()).await;

        assert!(result.is_success());
        assert_eq!(result.exit_code, Some(0));
        assert!(
            result
                .raw_stderr
                .contains("bash-logger: received Bash command"),
            "stderr should contain the logger message, got: {:?}",
            result.raw_stderr,
        );
    }

    // ===============================================================
    // f. test_config_loader_merges_plugin_hooks_with_user_hooks
    // ===============================================================

    #[tokio::test]
    async fn test_config_loader_merges_plugin_hooks_with_user_hooks() {
        // Simulate the merge that ForgeHookConfigLoader does by
        // building a MergedHooksConfig from two sources manually.
        let user_config: HooksConfig = serde_json::from_str(
            r#"{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo user-hook"}]}]}"#,
        )
        .unwrap();

        let plugin_config = load_fixture_hooks_config("dangerous-guard");

        // Build merged config.
        let mut merged = MergedHooksConfig::default();

        // User hooks.
        for (event, matchers) in user_config.0 {
            let entry = merged.entries.entry(event).or_default();
            for matcher in matchers {
                entry.push(HookMatcherWithSource {
                    matcher,
                    source: HookConfigSource::UserGlobal,
                    plugin_root: None,
                    plugin_name: None,
                    plugin_options: vec![],
                });
            }
        }

        // Plugin hooks.
        let plugin_root = fixture_plugin_path("dangerous-guard");
        for (event, matchers) in plugin_config.0 {
            let entry = merged.entries.entry(event).or_default();
            for matcher in matchers {
                entry.push(HookMatcherWithSource {
                    matcher,
                    source: HookConfigSource::Plugin,
                    plugin_root: Some(plugin_root.clone()),
                    plugin_name: Some("dangerous-guard".to_string()),
                    plugin_options: vec![],
                });
            }
        }

        // Assert both hooks present.
        assert_eq!(merged.total_matchers(), 2);
        let pre = merged.entries.get(&HookEventName::PreToolUse).unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0].source, HookConfigSource::UserGlobal);
        assert_eq!(pre[1].source, HookConfigSource::Plugin);
        assert_eq!(pre[1].plugin_name.as_deref(), Some("dangerous-guard"));
        assert_eq!(pre[1].plugin_root.as_deref(), Some(plugin_root.as_path()));
    }

    // ===============================================================
    // g. test_multi_plugin_hooks_execute_in_parallel
    // ===============================================================

    #[tokio::test]
    async fn test_multi_plugin_hooks_execute_in_parallel() {
        // Both bash-logger and dangerous-guard have PreToolUse hooks
        // matching "Bash".
        let logger_config = load_fixture_hooks_config("bash-logger");
        let guard_config = load_fixture_hooks_config("dangerous-guard");

        let logger_cmd = first_shell_command(&logger_config, &HookEventName::PreToolUse);
        let guard_cmd = first_shell_command(&guard_config, &HookEventName::PreToolUse);

        let input = pre_tool_use_input("Bash", json!({"command": "ls"}));

        // Run both hooks concurrently (simulating parallel dispatch).
        let (logger_result, guard_result) = tokio::join!(
            execute_shell_hook(&logger_cmd, &input, HashMap::new()),
            execute_shell_hook(&guard_cmd, &input, HashMap::new()),
        );

        // bash-logger: exit 0, stderr has logger output.
        assert!(logger_result.is_success());
        assert!(
            logger_result
                .raw_stderr
                .contains("bash-logger: received Bash command"),
            "bash-logger stderr: {:?}",
            logger_result.raw_stderr,
        );

        // dangerous-guard: exit 0 for safe 'ls' command.
        assert!(guard_result.is_success());
        assert_eq!(guard_result.exit_code, Some(0));
    }

    // ===============================================================
    // h. test_env_vars_substituted_in_hook_command
    // ===============================================================

    #[tokio::test]
    async fn test_env_vars_substituted_in_hook_command() {
        let temp = tempfile::TempDir::new().unwrap();
        let captured = temp.path().join("plugin-root.txt");

        // Create a hook command that uses ${FORGE_PLUGIN_ROOT}.
        let shell_cmd = ShellHookCommand {
            command: format!("echo ${{FORGE_PLUGIN_ROOT}} > {}", captured.display()),
            condition: None,
            shell: None,
            timeout: None,
            status_message: None,
            once: false,
            async_mode: false,
            async_rewake: false,
        };

        let input = pre_tool_use_input("Bash", json!({"command": "test"}));

        let mut env_vars = HashMap::new();
        let plugin_root = fixture_plugin_path("dangerous-guard");
        env_vars.insert(
            "FORGE_PLUGIN_ROOT".to_string(),
            plugin_root.display().to_string(),
        );

        let result = execute_shell_hook(&shell_cmd, &input, env_vars).await;

        assert!(result.is_success());

        let contents = std::fs::read_to_string(&captured).unwrap();
        assert_eq!(
            contents.trim(),
            plugin_root.display().to_string(),
            "FORGE_PLUGIN_ROOT should be substituted in the command"
        );
    }

    // ===============================================================
    // Additional: matcher integration tests using fixture configs
    // ===============================================================

    #[tokio::test]
    async fn test_dangerous_guard_matcher_only_matches_bash() {
        let config = load_fixture_hooks_config("dangerous-guard");
        let pattern = first_matcher_pattern(&config, &HookEventName::PreToolUse);

        assert_eq!(pattern, "Bash");
        assert!(matches_pattern(&pattern, "Bash"));
        assert!(!matches_pattern(&pattern, "Write"));
        assert!(!matches_pattern(&pattern, "Read"));
    }

    #[tokio::test]
    async fn test_config_watcher_matcher_matches_everything() {
        let config = load_fixture_hooks_config("config-watcher");
        let pattern = first_matcher_pattern(&config, &HookEventName::ConfigChange);

        assert_eq!(pattern, "*");
        assert!(matches_pattern(&pattern, "anything"));
        assert!(matches_pattern(&pattern, "SomeTool"));
    }

    #[tokio::test]
    async fn test_full_stack_sessionstart_hook_fires() {
        let config = load_fixture_hooks_config("full-stack");
        let shell_cmd = first_shell_command(&config, &HookEventName::SessionStart);

        // SessionStart input.
        let input = HookInput {
            base: HookInputBase {
                session_id: "sess-e2e".to_string(),
                transcript_path: PathBuf::from("/tmp/transcript.json"),
                cwd: PathBuf::from("/tmp"),
                permission_mode: None,
                agent_id: None,
                agent_type: None,
                hook_event_name: "SessionStart".to_string(),
            },
            payload: HookInputPayload::SessionStart {
                source: "user".to_string(),
                model: Some("claude-3-5-sonnet".to_string()),
            },
        };

        let result = execute_shell_hook(&shell_cmd, &input, HashMap::new()).await;

        assert!(result.is_success());
        assert_eq!(result.exit_code, Some(0));
        assert!(
            result
                .raw_stderr
                .contains("full-stack plugin session started"),
            "stderr should contain session-start message, got: {:?}",
            result.raw_stderr,
        );
    }

    #[tokio::test]
    async fn test_condition_matching_with_dangerous_guard() {
        // Verify matches_condition works with Bash tool and command patterns.
        let tool_input = json!({"command": "rm -rf /"});
        assert!(matches_condition("Bash", "Bash", &tool_input));
        assert!(matches_condition("Bash(rm *)", "Bash", &tool_input));
        assert!(!matches_condition("Bash(git *)", "Bash", &tool_input));
        assert!(!matches_condition("Write", "Bash", &tool_input));
    }
}
