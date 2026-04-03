//! Workspace and binary path helpers.
//!
//! Each smoke binary resolves paths relative to its own `CARGO_MANIFEST_DIR`.
//! These helpers centralise the logic so it isn't duplicated across binaries.

use std::path::PathBuf;

/// Returns the workspace root (two `parent()` calls above `CARGO_MANIFEST_DIR`
/// which is `crates/forge_shell_smoke`).
pub fn workspace_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR"); // …/crates/forge_shell_smoke
    PathBuf::from(manifest_dir)
        .parent() // …/crates
        .unwrap()
        .parent() // …/ (workspace root)
        .unwrap()
        .to_path_buf()
}

/// Returns the absolute path to the compiled `forge` debug binary.
pub fn forge_bin() -> PathBuf {
    let name = if cfg!(windows) { "forge.exe" } else { "forge" };
    workspace_root().join("target").join("debug").join(name)
}

/// Returns the absolute path to the forge ZSH plugin entry-point.
pub fn plugin_path() -> PathBuf {
    workspace_root()
        .join("shell-plugin")
        .join("forge.plugin.zsh")
}
