//! Smoke test for the Wave G-1 fixture plugin directory layout.
//!
//! This file validates that the 8 fixture plugins checked in under
//! `tests/fixtures/plugins/` have the shape expected by
//! `ForgePluginRepository` (defined in `forge_repo`). It is the
//! `forge_services`-side verification for Phase 11.1.1:
//!
//! - Every fixture directory exists.
//! - Every fixture has a parseable `.claude-plugin/plugin.json` that
//!   deserializes into a `PluginManifest` with the expected `name`.
//! - Every declared sibling directory (`hooks/`, `skills/`, `commands/`,
//!   `agents/`) exists when the plugin advertises that component type.
//! - The `full-stack` fixture has all component types plus a sibling
//!   `.mcp.json`.
//!
//! The full discovery-level integration tests (exercising
//! `ForgePluginRepository::scan_root` end-to-end) live in
//! `crates/forge_repo/src/plugin.rs`'s inline `#[cfg(test)] mod tests`
//! block because `ForgePluginRepository` is private to `forge_repo` and is
//! not re-exported through any public crate surface. See the Wave G-1
//! delivery report for the rationale behind that split.

mod common;

use std::path::Path;

use forge_domain::PluginManifest;

use crate::common::{
    FIXTURE_PLUGIN_NAMES, fixture_plugin_path, fixture_plugins_dir, list_fixture_plugin_names,
};

/// Parse the `.claude-plugin/plugin.json` manifest for a fixture plugin.
fn read_manifest(plugin_name: &str) -> PluginManifest {
    let path = fixture_plugin_path(plugin_name)
        .join(".claude-plugin")
        .join("plugin.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

fn assert_dir_nonempty(root: &Path, subdir: &str) {
    let dir = root.join(subdir);
    assert!(
        dir.is_dir(),
        "expected {} to be a directory, got {:?}",
        subdir,
        dir
    );
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to list {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .collect();
    assert!(
        !entries.is_empty(),
        "expected {} to contain at least one entry",
        dir.display()
    );
}

#[test]
fn test_all_eight_fixture_plugins_have_directories() {
    let root = fixture_plugins_dir();
    assert!(root.is_dir(), "fixture plugins root must exist: {:?}", root);

    assert_eq!(
        FIXTURE_PLUGIN_NAMES.len(),
        8,
        "Wave G-1 fixture catalog must list exactly 8 plugins"
    );
    assert_eq!(list_fixture_plugin_names().len(), 8);

    for name in FIXTURE_PLUGIN_NAMES {
        let dir = fixture_plugin_path(name);
        assert!(dir.is_dir(), "fixture plugin directory missing: {:?}", dir);
        let manifest_path = dir.join(".claude-plugin").join("plugin.json");
        assert!(
            manifest_path.is_file(),
            "plugin.json missing for {}: {:?}",
            name,
            manifest_path
        );
    }
}

#[test]
fn test_all_manifests_parse_and_names_match() {
    for name in FIXTURE_PLUGIN_NAMES {
        let manifest = read_manifest(name);
        assert_eq!(
            manifest.name.as_deref(),
            Some(*name),
            "manifest name must match the fixture directory name"
        );
        assert_eq!(
            manifest.version.as_deref(),
            Some("0.1.0"),
            "fixture plugins all use version 0.1.0"
        );
        assert!(
            manifest.description.is_some(),
            "{} manifest must have a description",
            name
        );
        assert!(
            manifest.author.is_some(),
            "{} manifest must have an author",
            name
        );
    }
}

#[test]
fn test_prettier_format_has_posttooluse_hooks_file() {
    let root = fixture_plugin_path("prettier-format");
    let hooks = root.join("hooks").join("hooks.json");
    let raw = std::fs::read_to_string(&hooks).expect("prettier-format hooks.json must exist");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("hooks.json must be JSON");
    assert!(
        value
            .get("hooks")
            .and_then(|h| h.get("PostToolUse"))
            .is_some(),
        "prettier-format must declare PostToolUse hooks"
    );
}

#[test]
fn test_bash_logger_has_pretooluse_bash_matcher() {
    let root = fixture_plugin_path("bash-logger");
    let hooks = root.join("hooks").join("hooks.json");
    let raw = std::fs::read_to_string(&hooks).expect("bash-logger hooks.json must exist");
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let matcher = value
        .pointer("/hooks/PreToolUse/0/matcher")
        .and_then(|v| v.as_str());
    assert_eq!(matcher, Some("Bash"));
}

#[test]
fn test_dangerous_guard_hook_reads_stdin() {
    // The hook input JSON arrives via stdin (see ForgeShellHookExecutor at
    // crates/forge_services/src/hook_runtime/shell.rs:73-112). The guard
    // must therefore read from stdin (via `cat`) rather than an env var.
    let root = fixture_plugin_path("dangerous-guard");
    let raw = std::fs::read_to_string(root.join("hooks").join("hooks.json")).expect("hooks.json");
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let command = value
        .pointer("/hooks/PreToolUse/0/hooks/0/command")
        .and_then(|v| v.as_str())
        .expect("dangerous-guard must declare a command string");
    assert!(
        command.contains("cat"),
        "dangerous-guard must read hook input from stdin via `cat`, got: {}",
        command
    );
    assert!(
        command.contains("rm -rf /"),
        "dangerous-guard must guard against `rm -rf /`, got: {}",
        command
    );
    assert!(
        command.contains("exit 2"),
        "dangerous-guard must exit 2 to signal a block, got: {}",
        command
    );
}

#[test]
fn test_skill_provider_has_three_skill_files() {
    let root = fixture_plugin_path("skill-provider");
    assert_dir_nonempty(&root, "skills");
    for skill in &[
        "inspect-code.md",
        "refactor-helper.md",
        "debug-assistant.md",
    ] {
        let path = root.join("skills").join(skill);
        assert!(path.is_file(), "missing skill: {:?}", path);
    }
}

#[test]
fn test_command_provider_has_two_command_files() {
    let root = fixture_plugin_path("command-provider");
    assert_dir_nonempty(&root, "commands");
    for cmd in &["greet.md", "status.md"] {
        let path = root.join("commands").join(cmd);
        assert!(path.is_file(), "missing command: {:?}", path);
    }
}

#[test]
fn test_agent_provider_has_security_reviewer() {
    let root = fixture_plugin_path("agent-provider");
    let agent = root.join("agents").join("security-reviewer.md");
    assert!(agent.is_file(), "missing agent file: {:?}", agent);
    let body = std::fs::read_to_string(&agent).unwrap();
    assert!(
        body.contains("security-reviewer"),
        "agent frontmatter must include the agent name"
    );
}

#[test]
fn test_full_stack_has_all_component_types() {
    let root = fixture_plugin_path("full-stack");
    // Component dirs.
    assert_dir_nonempty(&root, "skills");
    assert_dir_nonempty(&root, "commands");
    assert_dir_nonempty(&root, "agents");
    assert_dir_nonempty(&root, "hooks");
    // MCP sidecar at the plugin root (NOT mcp/.mcp.json — see
    // crates/forge_repo/src/plugin.rs:413 which hard-codes the sidecar
    // path as `<plugin_root>/.mcp.json`).
    //
    // The sidecar may use either Claude Code's camelCase key
    // (`mcpServers`) or the snake_case key (`mcp_servers`). Both are
    // accepted by `resolve_mcp_servers` via `McpJsonFile` which declares
    // `mcp_servers` with `alias = "mcpServers"`.
    let mcp = root.join(".mcp.json");
    assert!(
        mcp.is_file(),
        "full-stack must have a sibling .mcp.json sidecar at the plugin root, got {:?}",
        mcp
    );
    let mcp_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&mcp).unwrap()).unwrap();
    // The sidecar uses Claude Code's camelCase key (`mcpServers`), but
    // our `McpJsonFile` struct also accepts the snake_case alias. Check
    // whichever key the fixture actually uses.
    let servers = mcp_json
        .get("mcpServers")
        .or_else(|| mcp_json.get("mcp_servers"));
    assert!(
        servers.and_then(|v| v.get("full-stack-server")).is_some(),
        ".mcp.json must declare full-stack-server under mcpServers or mcp_servers key"
    );
}

#[test]
fn test_full_stack_hooks_has_sessionstart() {
    let root = fixture_plugin_path("full-stack");
    let raw = std::fs::read_to_string(root.join("hooks").join("hooks.json")).expect("hooks.json");
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        value
            .pointer("/hooks/SessionStart/0/matcher")
            .and_then(|v| v.as_str())
            == Some("*"),
        "full-stack must declare a SessionStart hook with matcher '*'"
    );
}

#[test]
fn test_config_watcher_declares_configchange_event() {
    let root = fixture_plugin_path("config-watcher");
    let raw = std::fs::read_to_string(root.join("hooks").join("hooks.json")).expect("hooks.json");
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(
        value.pointer("/hooks/ConfigChange/0").is_some(),
        "config-watcher must declare a ConfigChange hook array"
    );
}
