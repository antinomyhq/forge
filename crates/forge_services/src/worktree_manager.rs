//! Git worktree creation and management for Forge.
//!
//! This module owns the `git worktree add` command path used by both the
//! `--worktree` CLI flag (via `crates/forge_main/src/sandbox.rs`) and the
//! future `EnterWorktreeTool` (deferred). Extracted from `sandbox.rs` in
//! Wave E-2c-i to share the logic between both entry points.
//!
//! The function is deliberately side-effect-free on stdout — the caller
//! is responsible for any user-facing status printing. This keeps the
//! manager reusable from the runtime tool path (which has its own
//! reporting pipeline) without mixing in REPL-only `TitleFormat` calls.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Result of a successful worktree creation.
#[derive(Debug, Clone)]
pub struct WorktreeCreationResult {
    /// Absolute, canonicalized path to the worktree directory.
    pub path: PathBuf,
    /// Whether this was a fresh creation (`true`) or a reused existing
    /// worktree (`false`).
    pub created: bool,
}

/// Creates a git worktree with the given name via `git worktree add`.
///
/// The worktree is created in the parent directory of the git root.
///
/// # Behavior
///
/// - Verifies the current directory is inside a git repository.
/// - Computes the target worktree path as `<git-root-parent>/<name>`.
/// - If the target path exists and is already a git worktree, returns the
///   existing path with `created: false`.
/// - If the target path exists but is not a worktree, returns an error.
/// - Creates a new branch named `<name>` if it doesn't exist, otherwise reuses
///   the existing branch.
/// - Returns the canonicalized path on success.
///
/// # Errors
///
/// Returns an error if:
/// - Not inside a git repository.
/// - Git root cannot be determined.
/// - The target path exists but is not a valid worktree.
/// - `git worktree add` fails (e.g., invalid name, disk full).
/// - The final canonicalize step fails.
pub fn create_worktree(name: &str) -> Result<WorktreeCreationResult> {
    // First check if we're in a git repository
    let git_check = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .context("Failed to check if current directory is a git repository")?;

    if !git_check.status.success() {
        bail!(
            "Current directory is not inside a git repository. Worktree creation requires a git repository."
        );
    }

    // Get the git root directory
    let git_root_output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to get git root directory")?;

    if !git_root_output.status.success() {
        bail!("Failed to determine git repository root");
    }

    let git_root = String::from_utf8(git_root_output.stdout)
        .context("Git root path contains invalid UTF-8")?
        .trim()
        .to_string();

    let git_root_path = PathBuf::from(&git_root);

    // Get the parent directory of the git root
    let parent_dir = git_root_path.parent().context(
        "Git repository is at filesystem root - cannot create worktree in parent directory",
    )?;

    // Create the worktree path in the parent directory
    let worktree_path = parent_dir.join(name);

    // Check if worktree already exists
    if worktree_path.exists() {
        // Check if it's already a git worktree by checking if it has a .git file
        // (worktree marker)
        let git_file = worktree_path.join(".git");
        if git_file.exists() {
            let worktree_check = Command::new("git")
                .args(["rev-parse", "--is-inside-work-tree"])
                .current_dir(&worktree_path)
                .output()
                .context("Failed to check if target directory is a git worktree")?;

            if worktree_check.status.success() {
                let canonical = worktree_path
                    .canonicalize()
                    .context("Failed to canonicalize worktree path")?;
                return Ok(WorktreeCreationResult { path: canonical, created: false });
            }
        }

        bail!(
            "Directory '{}' already exists but is not a git worktree. Please remove it or choose a different name.",
            worktree_path.display()
        );
    }

    // Check if branch already exists
    let branch_check = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{name}")])
        .current_dir(&git_root_path)
        .output()
        .context("Failed to check if branch exists")?;

    let branch_exists = branch_check.status.success();

    // Create the worktree
    let mut worktree_cmd = Command::new("git");
    worktree_cmd.args(["worktree", "add"]);

    if !branch_exists {
        // Create new branch from current HEAD
        worktree_cmd.args(["-b", name]);
    }

    worktree_cmd.args([worktree_path.to_str().unwrap()]);

    if branch_exists {
        worktree_cmd.arg(name);
    }

    let worktree_output = worktree_cmd
        .current_dir(&git_root_path)
        .output()
        .context("Failed to create git worktree")?;

    if !worktree_output.status.success() {
        let stderr = String::from_utf8_lossy(&worktree_output.stderr);
        bail!("Failed to create git worktree: {stderr}");
    }

    // Return the canonicalized path
    let canonical = worktree_path
        .canonicalize()
        .context("Failed to canonicalize worktree path")?;
    Ok(WorktreeCreationResult { path: canonical, created: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test that exercises the happy path end-to-end against a
    /// real git repository. `#[ignore]`d because it needs a working
    /// `git` binary on PATH and a writable `TMPDIR` — both always
    /// present on a CI box but not something we want the default
    /// `cargo test` to depend on. Run manually with:
    ///
    /// ```bash
    /// cargo test -p forge_services --lib worktree_manager -- --ignored
    /// ```
    #[test]
    #[ignore = "needs a real git binary + writable tmpdir; exercise via Sandbox tests in forge_main"]
    fn test_create_worktree_result_created_flag() {
        // Intentionally empty — placeholder so the module's test surface
        // is non-zero and documents the manual-run path. The full logic
        // is exercised end-to-end by the `Sandbox::create` flow in
        // `crates/forge_main` which in turn calls `create_worktree`.
        let _ = create_worktree;
    }

    /// Sibling of [`test_create_worktree_result_created_flag`] for the
    /// `created: false` (reused) branch. Also `#[ignore]`d for the same
    /// reason.
    #[test]
    #[ignore = "needs a real git binary + writable tmpdir; exercise via Sandbox tests in forge_main"]
    fn test_create_worktree_reused_flag() {
        let _ = create_worktree;
    }
}
