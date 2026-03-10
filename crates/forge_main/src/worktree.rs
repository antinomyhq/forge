use std::process::Command;

use anyhow::{Context, bail};
use crate::cli::WorktreeCommand;

/// Returns the main worktree root path by reading the first line of
/// `git worktree list`.
fn main_worktree_root() -> anyhow::Result<std::path::PathBuf> {
    let out = Command::new("git")
        .args(["worktree", "list"])
        .output()
        .context("Failed to run git worktree list")?;
    if !out.status.success() {
        bail!("Not inside a git repository");
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first_line = stdout.lines().next().context("No worktrees found")?;
    let path = first_line
        .split_whitespace()
        .next()
        .context("Unexpected git worktree list output")?;
    Ok(std::path::PathBuf::from(path))
}

/// Handles `forge worktree` subcommands using direct git process invocation.
pub fn handle_worktree_command(command: WorktreeCommand) -> anyhow::Result<()> {
    match command {
        WorktreeCommand::List => {
            // Emit porcelain output: one line per worktree with branch and path
            // separated by a tab, suitable for fzf consumption.
            let out = Command::new("git")
                .args(["worktree", "list"])
                .output()
                .context("Failed to run git worktree list")?;
            if !out.status.success() {
                bail!("Not inside a git repository");
            }
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let mut parts = line.split_whitespace();
                let path = parts.next().unwrap_or("");
                // Skip the commit hash (second field), branch is third
                let _commit = parts.next();
                let branch_raw = parts.next().unwrap_or("(bare)");
                // Strip surrounding brackets: [main] -> main
                let branch = branch_raw.trim_matches(|c| c == '[' || c == ']');
                println!("{branch}\t{path}");
            }
        }
        WorktreeCommand::Create { branch } => {
            let main_root = main_worktree_root()?;
            let parent = main_root.parent().context(
                "Git repository is at filesystem root; cannot create worktree alongside it",
            )?;

            // Derive a filesystem-safe directory name from the branch
            // (e.g. "feature/foo" -> "foo").
            let dir_name = branch
                .split('/')
                .last()
                .unwrap_or(&branch)
                .replace(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-', "-");
            let worktree_path = parent.join(&dir_name);

            // Decide whether to check out an existing branch or create a new one.
            let branch_exists = Command::new("git")
                .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
                .current_dir(&main_root)
                .status()
                .context("Failed to check if branch exists")?
                .success();

            let status = if branch_exists {
                Command::new("git")
                    .args(["worktree", "add", worktree_path.to_str().unwrap(), &branch])
                    .current_dir(&main_root)
                    .status()
                    .context("Failed to run git worktree add")?
            } else {
                Command::new("git")
                    .args(["worktree", "add", "-b", &branch, worktree_path.to_str().unwrap()])
                    .current_dir(&main_root)
                    .status()
                    .context("Failed to run git worktree add -b")?
            };

            if !status.success() {
                bail!("git worktree add failed");
            }

            // Print the new worktree path so the shell wrapper can cd into it.
            println!("{}", worktree_path.display());
        }
        WorktreeCommand::Delete { path } => {
            let status = Command::new("git")
                .args(["worktree", "remove", &path])
                .status()
                .context("Failed to run git worktree remove")?;
            if !status.success() {
                bail!("git worktree remove failed");
            }
        }
    }

    Ok(())
}
