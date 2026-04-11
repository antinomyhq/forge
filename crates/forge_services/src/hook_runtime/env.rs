//! Test-only reference implementation for FORGE_* environment variables.
//!
//! Production env var construction is done inline in the dispatcher
//! (`forge_app::hooks::plugin`) because `forge_app` cannot depend on
//! `forge_services`. This module exists purely as a readable reference
//! implementation and as a test helper for verifying env var logic.
//!
//! Mirrors Claude Code's `prepareEnv` at
//! `claude-code/src/utils/hooks.ts:882-909` but uses Forge's `FORGE_`
//! prefix and a Forge-specific plugin-data layout.

use std::collections::HashMap;
use std::path::Path;

/// Build the `FORGE_*` environment variable map for a hook subprocess.
///
/// Keys produced (when the corresponding input is provided):
///
/// - `FORGE_PROJECT_DIR` — stable project root (not the worktree path).
/// - `FORGE_PLUGIN_ROOT` — path to the current plugin's directory (only set
///   when the hook originates from a plugin).
/// - `FORGE_PLUGIN_DATA` — `<forge_home>/plugin-data/<plugin-name>/` (only set
///   when `plugin_name` is provided). The caller is responsible for creating
///   this directory.
/// - `FORGE_PLUGIN_OPTION_<KEY>` — one per user-configured plugin option. Keys
///   are upper-cased and hyphens are replaced with underscores.
/// - `FORGE_SESSION_ID` — current session ID.
/// - `FORGE_ENV_FILE` — temp file path that `SessionStart`/`Setup` hooks write
///   `export FOO=bar` lines into.
///
/// `plugin_options` is a slice of `(key, value)` pairs rather than a
/// `HashMap` so the caller controls iteration order (useful for
/// deterministic test assertions).
fn build_hook_env_vars(
    project_dir: &Path,
    plugin_root: Option<&Path>,
    plugin_name: Option<&str>,
    plugin_options: &[(String, String)],
    session_id: &str,
    env_file: &Path,
    forge_home: &Path,
) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    vars.insert(
        "FORGE_PROJECT_DIR".to_string(),
        project_dir.display().to_string(),
    );

    if let Some(root) = plugin_root {
        vars.insert("FORGE_PLUGIN_ROOT".to_string(), root.display().to_string());
    }

    if let Some(name) = plugin_name {
        let data_dir = forge_home.join("plugin-data").join(name);
        vars.insert(
            "FORGE_PLUGIN_DATA".to_string(),
            data_dir.display().to_string(),
        );
    }

    for (key, val) in plugin_options {
        let env_key = format!(
            "FORGE_PLUGIN_OPTION_{}",
            key.to_uppercase().replace('-', "_")
        );
        vars.insert(env_key, val.clone());
    }

    vars.insert("FORGE_SESSION_ID".to_string(), session_id.to_string());
    vars.insert("FORGE_ENV_FILE".to_string(), env_file.display().to_string());

    vars
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_minimal_inputs_produce_three_core_vars() {
        let actual = build_hook_env_vars(
            Path::new("/proj"),
            None,
            None,
            &[],
            "sess-1",
            Path::new("/tmp/env"),
            Path::new("/home/u/.forge"),
        );

        assert_eq!(
            actual.get("FORGE_PROJECT_DIR").map(String::as_str),
            Some("/proj")
        );
        assert_eq!(
            actual.get("FORGE_SESSION_ID").map(String::as_str),
            Some("sess-1")
        );
        assert_eq!(
            actual.get("FORGE_ENV_FILE").map(String::as_str),
            Some("/tmp/env")
        );
        assert!(!actual.contains_key("FORGE_PLUGIN_ROOT"));
        assert!(!actual.contains_key("FORGE_PLUGIN_DATA"));
    }

    #[test]
    fn test_plugin_name_produces_plugin_data_path() {
        let forge_home = PathBuf::from("/home/u/.forge");
        let actual = build_hook_env_vars(
            Path::new("/proj"),
            Some(Path::new("/plugins/demo")),
            Some("demo"),
            &[],
            "sess-1",
            Path::new("/tmp/env"),
            &forge_home,
        );

        assert_eq!(
            actual.get("FORGE_PLUGIN_ROOT").map(String::as_str),
            Some("/plugins/demo")
        );
        assert_eq!(
            actual.get("FORGE_PLUGIN_DATA").map(String::as_str),
            Some("/home/u/.forge/plugin-data/demo")
        );
    }

    #[test]
    fn test_plugin_options_are_upper_cased_and_hyphens_normalized() {
        let options = vec![
            ("api-key".to_string(), "secret".to_string()),
            ("log-level".to_string(), "debug".to_string()),
        ];

        let actual = build_hook_env_vars(
            Path::new("/proj"),
            None,
            None,
            &options,
            "sess",
            Path::new("/tmp/env"),
            Path::new("/home/u/.forge"),
        );

        assert_eq!(
            actual
                .get("FORGE_PLUGIN_OPTION_API_KEY")
                .map(String::as_str),
            Some("secret")
        );
        assert_eq!(
            actual
                .get("FORGE_PLUGIN_OPTION_LOG_LEVEL")
                .map(String::as_str),
            Some("debug")
        );
    }
}
