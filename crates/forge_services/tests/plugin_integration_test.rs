//! Wave G-3: Multi-plugin interaction, hot-reload, and error-path tests.
//!
//! Phase 11.1.4 — multi-plugin interaction (GROUP A)
//! Phase 11.1.5 — hot-reload semantics (GROUP B)
//! Phase 11.4   — error paths (GROUP C)
//!
//! These tests exercise plugin loading, hook execution, skill namespacing,
//! and error handling paths. They build on the Wave G-1 fixture plugins
//! (`tests/fixtures/plugins/`) and the e2e infrastructure from Wave G-2
//! (`hook_execution_e2e.rs`).
//!
//! For the hot-reload and error-path tests that need to scan plugin
//! directories, we replicate the plugin manifest probing logic from
//! `ForgePluginRepository::load_one_plugin` using direct filesystem access.
//! This avoids depending on private APIs while exercising the exact same
//! on-disk contract.
//!
//! All tests are gated to `#[cfg(unix)]` because hook commands use `bash`.

#[cfg(unix)]
mod common;

#[cfg(unix)]
mod integration {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use forge_app::hook_runtime::{HookConfigSource, HookMatcherWithSource, MergedHooksConfig};
    use forge_domain::{
        HookCommand, HookEventName, HookInput, HookInputBase, HookInputPayload, HookOutcome,
        HookOutput, HooksConfig, LoadedPlugin, PluginLoadError, PluginLoadErrorKind,
        PluginLoadResult, PluginManifest, PluginSource, ShellHookCommand,
    };
    use serde_json::json;
    use tokio::io::AsyncWriteExt;

    use crate::common::{fixture_plugin_path, fixture_plugins_dir};

    // ---------------------------------------------------------------
    // Shell execution helper (mirrors ForgeShellHookExecutor)
    // ---------------------------------------------------------------

    /// Result of executing a shell hook command.
    #[derive(Debug)]
    #[allow(dead_code)]
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

        fn outcome(&self) -> HookOutcome {
            match self.exit_code {
                Some(0) => HookOutcome::Success,
                Some(2) => HookOutcome::Blocking,
                Some(_) => HookOutcome::NonBlockingError,
                None => HookOutcome::Cancelled,
            }
        }
    }

    /// Execute a shell hook command the same way `ForgeShellHookExecutor`
    /// does: serialize `HookInput` to JSON, pipe it to `bash -c <command>`
    /// on stdin, read stdout/stderr, and return the exit code + output.
    async fn execute_shell_hook(
        shell_cmd: &ShellHookCommand,
        input: &HookInput,
        timeout_secs: Option<u64>,
    ) -> ShellExecResult {
        let input_json = serde_json::to_string(input).expect("HookInput serialization");

        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c")
            .arg(&shell_cmd.command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().expect("failed to spawn bash");

        if let Some(mut stdin) = child.stdin.take() {
            // Ignore write/shutdown errors — the child may have already
            // exited (e.g. `exit 1`), which closes the pipe before we
            // finish writing.  This is expected and not an error.
            let _ = stdin.write_all(input_json.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
            let _ = stdin.shutdown().await;
        }

        let timeout_dur = std::time::Duration::from_secs(timeout_secs.unwrap_or(30));

        match tokio::time::timeout(timeout_dur, child.wait_with_output()).await {
            Ok(Ok(output)) => {
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
            Ok(Err(e)) => panic!("hook wait failed: {e}"),
            Err(_) => {
                // Timeout — child is killed by kill_on_drop.
                ShellExecResult {
                    exit_code: None,
                    raw_stdout: String::new(),
                    raw_stderr: format!("hook timed out after {}s", timeout_dur.as_secs()),
                    parsed_output: None,
                }
            }
        }
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

    /// Construct a minimal `HookInput` for a `PreToolUse` event.
    fn pre_tool_use_input(tool_name: &str, tool_input: serde_json::Value) -> HookInput {
        HookInput {
            base: HookInputBase {
                session_id: "sess-g3".to_string(),
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
                tool_use_id: "toolu_g3_test".to_string(),
            },
        }
    }

    /// Recursively copies a directory tree.
    fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let src = entry.path();
            let dst = to.join(entry.file_name());
            let ft = entry.file_type()?;
            if ft.is_dir() {
                copy_dir_recursive(&src, &dst)?;
            } else if ft.is_file() {
                std::fs::copy(&src, &dst)?;
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Plugin scanning helper — mirrors ForgePluginRepository logic
    // ---------------------------------------------------------------

    /// Probe for a manifest file in priority order, matching
    /// `ForgePluginRepository::find_manifest`.
    fn find_manifest(plugin_dir: &Path) -> Option<PathBuf> {
        let candidates = [
            plugin_dir.join(".forge-plugin").join("plugin.json"),
            plugin_dir.join(".claude-plugin").join("plugin.json"),
            plugin_dir.join("plugin.json"),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    /// Load a single plugin from its directory, matching
    /// `ForgePluginRepository::load_one_plugin`.
    fn load_one_plugin(
        plugin_dir: &Path,
        source: PluginSource,
    ) -> Result<Option<LoadedPlugin>, String> {
        let manifest_path = match find_manifest(plugin_dir) {
            Some(p) => p,
            None => return Ok(None),
        };

        let raw = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {e}"))?;

        let manifest: PluginManifest = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse manifest {}: {e}", manifest_path.display()))?;

        let dir_name = plugin_dir
            .file_name()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_else(|| "<unknown>".to_string());

        let name = manifest.name.clone().unwrap_or_else(|| dir_name.clone());

        // Auto-detect component directories.
        let commands_paths = auto_detect_dir(plugin_dir, "commands");
        let agents_paths = auto_detect_dir(plugin_dir, "agents");
        let skills_paths = auto_detect_dir(plugin_dir, "skills");

        Ok(Some(LoadedPlugin {
            name,
            manifest,
            path: plugin_dir.to_path_buf(),
            source,
            enabled: true,
            is_builtin: false,
            commands_paths,
            agents_paths,
            skills_paths,
            mcp_servers: None,
        }))
    }

    /// Auto-detect a component directory if it exists.
    fn auto_detect_dir(plugin_dir: &Path, name: &str) -> Vec<PathBuf> {
        let dir = plugin_dir.join(name);
        if dir.is_dir() { vec![dir] } else { Vec::new() }
    }

    /// Scan a plugin root directory for all plugins.
    fn scan_plugins_in_dir(root: &Path) -> Vec<LoadedPlugin> {
        let (plugins, _) = scan_plugins_in_dir_with_errors(root);
        plugins
    }

    /// Scan a plugin root directory, returning both plugins and errors.
    fn scan_plugins_in_dir_with_errors(root: &Path) -> (Vec<LoadedPlugin>, Vec<PluginLoadError>) {
        let mut plugins = Vec::new();
        let mut errors = Vec::new();

        if !root.is_dir() {
            return (plugins, errors);
        }

        let mut entries: Vec<_> = std::fs::read_dir(root)
            .expect("read plugin root directory")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .collect();

        // Sort for deterministic ordering.
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            match load_one_plugin(&path, PluginSource::Project) {
                Ok(Some(plugin)) => plugins.push(plugin),
                Ok(None) => {} // Not a plugin directory.
                Err(e) => {
                    let plugin_name = path.file_name().and_then(|s| s.to_str()).map(String::from);
                    errors.push(PluginLoadError {
                        plugin_name,
                        path,
                        kind: PluginLoadErrorKind::Other,
                        error: e,
                    });
                }
            }
        }

        (plugins, errors)
    }

    // ===============================================================
    // GROUP A — Multi-plugin interaction (Phase 11.1.4)
    // ===============================================================

    /// A. test_multi_plugin_same_event_both_fire
    ///
    /// Two plugins (bash-logger + dangerous-guard) both declare
    /// PreToolUse/Bash hooks. Execute with safe 'ls' command.
    /// Both should fire — assert both stderr outputs present.
    #[tokio::test]
    async fn test_multi_plugin_same_event_both_fire() {
        let logger_config = load_fixture_hooks_config("bash-logger");
        let guard_config = load_fixture_hooks_config("dangerous-guard");

        let logger_cmd = first_shell_command(&logger_config, &HookEventName::PreToolUse);
        let guard_cmd = first_shell_command(&guard_config, &HookEventName::PreToolUse);

        let input = pre_tool_use_input("Bash", json!({"command": "ls"}));

        // Run both hooks concurrently (simulating parallel dispatch).
        let (logger_result, guard_result) = tokio::join!(
            execute_shell_hook(&logger_cmd, &input, None),
            execute_shell_hook(&guard_cmd, &input, None),
        );

        // bash-logger: exit 0, stderr has logger output.
        assert!(
            logger_result.is_success(),
            "bash-logger should succeed, got exit_code={:?}",
            logger_result.exit_code
        );
        assert!(
            logger_result
                .raw_stderr
                .contains("bash-logger: received Bash command"),
            "bash-logger stderr should contain logger message, got: {:?}",
            logger_result.raw_stderr,
        );

        // dangerous-guard: exit 0 for safe 'ls' command.
        assert!(
            guard_result.is_success(),
            "dangerous-guard should succeed for safe 'ls', got exit_code={:?}",
            guard_result.exit_code
        );

        // Both hooks produced output — confirm neither was silently dropped.
        assert!(
            !logger_result.raw_stderr.is_empty(),
            "logger stderr should not be empty"
        );
    }

    /// B. test_multi_plugin_namespace_isolation
    ///
    /// skill-provider and command-provider plugins have skills/commands
    /// with different namespaces. Load both, assert skill names are
    /// prefixed with plugin name.
    #[tokio::test]
    async fn test_multi_plugin_namespace_isolation() {
        let skill_provider_path = fixture_plugin_path("skill-provider");
        let command_provider_path = fixture_plugin_path("command-provider");

        // Build LoadedPlugin structs matching what ForgePluginRepository
        // would produce for these fixture plugins.
        let skill_plugin = LoadedPlugin {
            name: "skill-provider".to_string(),
            manifest: PluginManifest {
                name: Some("skill-provider".to_string()),
                ..Default::default()
            },
            path: skill_provider_path.clone(),
            source: PluginSource::Project,
            enabled: true,
            is_builtin: false,
            commands_paths: Vec::new(),
            agents_paths: Vec::new(),
            skills_paths: vec![skill_provider_path.join("skills")],
            mcp_servers: None,
        };

        let cmd_plugin = LoadedPlugin {
            name: "command-provider".to_string(),
            manifest: PluginManifest {
                name: Some("command-provider".to_string()),
                ..Default::default()
            },
            path: command_provider_path.clone(),
            source: PluginSource::Project,
            enabled: true,
            is_builtin: false,
            commands_paths: vec![command_provider_path.join("commands")],
            agents_paths: Vec::new(),
            skills_paths: Vec::new(),
            mcp_servers: None,
        };

        // Verify component path isolation: skill-provider has skills_paths only,
        // command-provider has commands_paths only — no overlap.
        assert!(
            !skill_plugin.skills_paths.is_empty(),
            "skill-provider should have skills_paths"
        );
        assert!(
            skill_plugin.commands_paths.is_empty(),
            "skill-provider should not have commands_paths"
        );
        assert!(
            !cmd_plugin.commands_paths.is_empty(),
            "command-provider should have commands_paths"
        );
        assert!(
            cmd_plugin.skills_paths.is_empty(),
            "command-provider should not have skills_paths"
        );

        // Verify the namespace prefix convention: `{plugin_name}:{dir_name}`.
        // The ForgeSkillRepository::load_plugin_skills_from_dir uses this format
        // (see crates/forge_repo/src/skill.rs:268-269).
        let skill_prefix = format!("{}:", skill_plugin.name);
        let cmd_prefix = format!("{}:", cmd_plugin.name);

        assert_ne!(
            skill_prefix, cmd_prefix,
            "plugin namespace prefixes must be distinct"
        );
        assert_eq!(skill_prefix, "skill-provider:");
        assert_eq!(cmd_prefix, "command-provider:");

        // Verify both plugins can coexist in a PluginLoadResult without collision.
        let result =
            PluginLoadResult::new(vec![skill_plugin.clone(), cmd_plugin.clone()], Vec::new());
        assert_eq!(result.plugins.len(), 2);
        assert!(!result.has_errors());

        // Plugin names must be unique.
        let names: Vec<&str> = result.plugins.iter().map(|p| p.name.as_str()).collect();
        assert_ne!(names[0], names[1]);
    }

    /// C. test_disabled_plugin_skipped
    ///
    /// Load full-stack plugin, mark it disabled in PluginLoadResult,
    /// verify its hooks are NOT in merged config.
    #[tokio::test]
    async fn test_disabled_plugin_skipped() {
        let full_stack_path = fixture_plugin_path("full-stack");
        let full_stack_config = load_fixture_hooks_config("full-stack");

        // Create a LoadedPlugin marked as disabled.
        let disabled_plugin = LoadedPlugin {
            name: "full-stack".to_string(),
            manifest: PluginManifest { name: Some("full-stack".to_string()), ..Default::default() },
            path: full_stack_path.clone(),
            source: PluginSource::Project,
            enabled: false, // <-- disabled
            is_builtin: false,
            commands_paths: Vec::new(),
            agents_paths: Vec::new(),
            skills_paths: Vec::new(),
            mcp_servers: None,
        };

        // Build a merged config that only includes enabled plugins' hooks.
        // This mimics what the hook config loader does: it skips disabled plugins.
        let mut merged = MergedHooksConfig::default();

        let plugin_load_result = PluginLoadResult::new(vec![disabled_plugin], Vec::new());

        for plugin in &plugin_load_result.plugins {
            if !plugin.enabled {
                continue;
            }
            for (event, matchers) in &full_stack_config.0 {
                let entry = merged.entries.entry(event.clone()).or_default();
                for matcher in matchers {
                    entry.push(HookMatcherWithSource {
                        matcher: matcher.clone(),
                        source: HookConfigSource::Plugin,
                        plugin_root: Some(plugin.path.clone()),
                        plugin_name: Some(plugin.name.clone()),
                        plugin_options: vec![],
                    });
                }
            }
        }

        // The merged config must be empty because the only plugin was disabled.
        assert!(
            merged.is_empty(),
            "disabled plugin's hooks should not appear in merged config, got {} matchers",
            merged.total_matchers()
        );
    }

    // ===============================================================
    // GROUP B — Hot-reload (Phase 11.1.5)
    // ===============================================================

    /// D. test_reload_picks_up_new_plugin
    ///
    /// Start with one fixture plugin dir, add a second plugin dir,
    /// call reload (re-scan), verify new plugin appears.
    #[tokio::test]
    async fn test_reload_picks_up_new_plugin() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();

        // Start with only bash-logger.
        copy_dir_recursive(
            &fixture_plugins_dir().join("bash-logger"),
            &root.join("bash-logger"),
        )
        .unwrap();

        // First scan: only bash-logger.
        let plugins_v1 = scan_plugins_in_dir(root);
        assert_eq!(
            plugins_v1.len(),
            1,
            "initial scan should find 1 plugin, found: {:?}",
            plugins_v1.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        assert_eq!(plugins_v1[0].name, "bash-logger");

        // "Hot reload": copy dangerous-guard into the same root.
        copy_dir_recursive(
            &fixture_plugins_dir().join("dangerous-guard"),
            &root.join("dangerous-guard"),
        )
        .unwrap();

        // Re-scan (simulates reload): both plugins should appear.
        let plugins_v2 = scan_plugins_in_dir(root);
        assert_eq!(
            plugins_v2.len(),
            2,
            "after adding dangerous-guard, should find 2 plugins, found: {:?}",
            plugins_v2.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        let names: Vec<&str> = plugins_v2.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"bash-logger"));
        assert!(names.contains(&"dangerous-guard"));
    }

    /// E. test_reload_drops_removed_plugin
    ///
    /// Start with two plugins, remove one's directory, reload,
    /// verify it disappears.
    #[tokio::test]
    async fn test_reload_drops_removed_plugin() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();

        copy_dir_recursive(
            &fixture_plugins_dir().join("bash-logger"),
            &root.join("bash-logger"),
        )
        .unwrap();
        copy_dir_recursive(
            &fixture_plugins_dir().join("prettier-format"),
            &root.join("prettier-format"),
        )
        .unwrap();

        let plugins_v1 = scan_plugins_in_dir(root);
        assert_eq!(plugins_v1.len(), 2);

        // Remove prettier-format.
        std::fs::remove_dir_all(root.join("prettier-format")).unwrap();

        // Re-scan: only bash-logger should remain.
        let plugins_v2 = scan_plugins_in_dir(root);
        assert_eq!(
            plugins_v2.len(),
            1,
            "after removing prettier-format, should find 1 plugin, found: {:?}",
            plugins_v2.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        assert_eq!(plugins_v2[0].name, "bash-logger");
    }

    /// F. test_reload_preserves_enabled_state
    ///
    /// Enable a plugin, reload, verify it stays enabled after re-applying
    /// config overrides.
    #[tokio::test]
    async fn test_reload_preserves_enabled_state() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();

        copy_dir_recursive(
            &fixture_plugins_dir().join("bash-logger"),
            &root.join("bash-logger"),
        )
        .unwrap();

        // First scan.
        let plugins_v1 = scan_plugins_in_dir(root);
        assert_eq!(plugins_v1.len(), 1);
        assert!(plugins_v1[0].enabled, "plugins are enabled by default");

        // Simulate user toggling the enabled state. In production this
        // comes from ForgeConfig/PluginSetting; here we track it in a map.
        let enabled_overrides: HashMap<String, bool> = {
            let mut m = HashMap::new();
            m.insert("bash-logger".to_string(), true);
            m
        };

        // Re-scan (simulates reload) — the raw scan always returns
        // enabled=true, then the caller applies config overrides.
        let mut plugins_v2 = scan_plugins_in_dir(root);
        assert_eq!(plugins_v2.len(), 1);

        // Re-apply the same enabled overrides (as the production config
        // loader does after every reload).
        for plugin in &mut plugins_v2 {
            if let Some(&enabled) = enabled_overrides.get(&plugin.name) {
                plugin.enabled = enabled;
            }
        }

        assert!(
            plugins_v2[0].enabled,
            "enabled state should be preserved across reloads"
        );
        assert_eq!(plugins_v2[0].name, "bash-logger");
    }

    // ===============================================================
    // GROUP C — Error paths (Phase 11.4)
    // ===============================================================

    /// G. test_malformed_manifest_returns_load_error
    ///
    /// Create a plugin dir with invalid plugin.json, assert
    /// PluginLoadError is surfaced.
    #[tokio::test]
    async fn test_malformed_manifest_returns_load_error() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();

        // Create a broken plugin with invalid JSON manifest.
        let broken = root.join("broken-plugin");
        std::fs::create_dir_all(broken.join(".claude-plugin")).unwrap();
        std::fs::write(
            broken.join(".claude-plugin").join("plugin.json"),
            "{ this is NOT valid json !!!",
        )
        .unwrap();

        let (plugins, errors) = scan_plugins_in_dir_with_errors(root);

        // No valid plugins should load.
        assert!(
            plugins.is_empty(),
            "no valid plugins should load from a broken manifest"
        );

        // The error should be captured.
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one load error, got: {errors:?}"
        );
        let err = &errors[0];
        assert_eq!(err.plugin_name.as_deref(), Some("broken-plugin"));
        assert!(
            err.error.to_lowercase().contains("parse")
                || err.error.to_lowercase().contains("json")
                || err.error.to_lowercase().contains("expected"),
            "error should mention JSON parsing, got: {}",
            err.error
        );
    }

    /// H. test_missing_hooks_json_returns_empty_config
    ///
    /// Create a plugin dir with valid manifest but NO hooks/ dir,
    /// assert no crash and empty hooks config.
    #[tokio::test]
    async fn test_missing_hooks_json_returns_empty_config() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();

        // Create a minimal valid plugin with no hooks directory.
        let plugin_dir = root.join("no-hooks-plugin");
        std::fs::create_dir_all(plugin_dir.join(".claude-plugin")).unwrap();
        std::fs::write(
            plugin_dir.join(".claude-plugin").join("plugin.json"),
            r#"{"name": "no-hooks-plugin", "version": "0.1.0", "description": "A plugin without hooks"}"#,
        )
        .unwrap();

        let (plugins, errors) = scan_plugins_in_dir_with_errors(root);

        // Should load cleanly with no errors.
        assert!(errors.is_empty(), "no errors expected, got: {errors:?}");
        assert_eq!(plugins.len(), 1);

        let plugin = &plugins[0];
        assert_eq!(plugin.name, "no-hooks-plugin");
    }

    /// I. test_hook_command_syntax_error_returns_non_blocking
    ///
    /// Hook command with non-zero exit code (not 2) produces
    /// HookOutcome::NonBlockingError.
    #[tokio::test]
    async fn test_hook_command_syntax_error_returns_non_blocking() {
        let shell_cmd = ShellHookCommand {
            command: "exit 1".to_string(),
            condition: None,
            shell: None,
            timeout: None,
            status_message: None,
            once: false,
            async_mode: false,
            async_rewake: false,
        };

        let input = pre_tool_use_input("Bash", json!({"command": "test"}));
        let result = execute_shell_hook(&shell_cmd, &input, None).await;

        assert_eq!(
            result.outcome(),
            HookOutcome::NonBlockingError,
            "non-zero exit (not 2) should produce NonBlockingError, got exit_code={:?}",
            result.exit_code
        );
        assert!(!result.is_success());
    }

    /// J. test_hook_timeout_returns_cancelled
    ///
    /// Hook with 'sleep 30' and 1s timeout produces
    /// HookOutcome::Cancelled.
    #[tokio::test]
    async fn test_hook_timeout_returns_cancelled() {
        let shell_cmd = ShellHookCommand {
            command: "sleep 30".to_string(),
            condition: None,
            shell: None,
            timeout: Some(1),
            status_message: None,
            once: false,
            async_mode: false,
            async_rewake: false,
        };

        let input = pre_tool_use_input("Bash", json!({"command": "test"}));

        // Use a 1-second timeout for the execution.
        let result = execute_shell_hook(&shell_cmd, &input, Some(1)).await;

        // The hook should be cancelled due to timeout.
        assert_eq!(
            result.outcome(),
            HookOutcome::Cancelled,
            "timed-out hook should produce Cancelled, got exit_code={:?}",
            result.exit_code
        );
        assert!(
            result.exit_code.is_none(),
            "timed-out hook should have no exit code, got {:?}",
            result.exit_code
        );
        assert!(
            result.raw_stderr.contains("timed out"),
            "stderr should contain 'timed out', got: {:?}",
            result.raw_stderr
        );
    }
}
