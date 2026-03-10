//! Utility functions for the ZSH setup orchestrator.
//!
//! Provides command execution helpers, path conversion utilities,
//! version comparison, and other shared infrastructure used across
//! the setup submodules.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use super::types::SudoCapability;

/// Checks if a command exists on the system using POSIX-compliant
/// `command -v` (available on all Unix shells) or `where` on Windows.
///
/// Returns the resolved path if the command is found, `None` otherwise.
pub async fn resolve_command_path(cmd: &str) -> Option<String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("where")
            .arg(cmd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .await
            .ok()?
    } else {
        Command::new("sh")
            .args(["-c", &format!("command -v {cmd}")])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .await
            .ok()?
    };

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() { None } else { Some(path) }
    } else {
        None
    }
}

/// Returns `true` if the given command is available on the system.
pub(super) async fn command_exists(cmd: &str) -> bool {
    resolve_command_path(cmd).await.is_some()
}

/// Runs a command, optionally prepending `sudo`, and returns the result.
///
/// # Arguments
///
/// * `program` - The program to run
/// * `args` - Arguments to pass
/// * `sudo` - The sudo capability level
///
/// # Errors
///
/// Returns error if:
/// - Sudo is needed but not available
/// - The command fails to spawn or exits with non-zero status
pub(super) async fn run_maybe_sudo(
    program: &str,
    args: &[&str],
    sudo: &SudoCapability,
) -> Result<()> {
    let mut cmd = match sudo {
        SudoCapability::Root | SudoCapability::NoneNeeded => {
            let mut c = Command::new(program);
            c.args(args);
            c
        }
        SudoCapability::SudoAvailable => {
            let mut c = Command::new("sudo");
            c.arg(program);
            c.args(args);
            c
        }
        SudoCapability::NoneAvailable => {
            bail!("Root privileges required to install zsh. Either run as root or install sudo.");
        }
    };

    cmd.stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit());

    let status = cmd
        .status()
        .await
        .context(format!("Failed to execute {}", program))?;

    if !status.success() {
        bail!("{} exited with code {:?}", program, status.code());
    }

    Ok(())
}

/// Runs a command in a given working directory, suppressing stdout/stderr.
pub(super) async fn run_cmd(program: &str, args: &[&str], cwd: &Path) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .context(format!("Failed to run {}", program))?;

    if !status.success() {
        bail!("{} failed with exit code {:?}", program, status.code());
    }
    Ok(())
}

/// Converts a path to a string, using lossy conversion.
pub(super) fn path_str(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

/// Converts a Unix-style path to a Windows path.
///
/// Performs manual `/c/...` -> `C:\...` conversion for Git Bash environments.
pub(super) fn to_win_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    // Simple conversion: /c/Users/... -> C:\Users\...
    if s.len() >= 3 && s.starts_with('/') && s.chars().nth(2) == Some('/') {
        let drive = s.chars().nth(1).unwrap().to_uppercase().to_string();
        let rest = &s[2..];
        format!("{}:{}", drive, rest.replace('/', "\\"))
    } else {
        s.replace('/', "\\")
    }
}

/// Recursively searches for a file by name in a directory.
pub(super) async fn find_file_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return None,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == name).unwrap_or(false) {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = Box::pin(find_file_recursive(&path, name)).await
        {
            return Some(found);
        }
    }

    None
}

/// Resolves the path to the zsh binary.
pub(super) async fn resolve_zsh_path() -> String {
    let which = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    match Command::new(which)
        .arg("zsh")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout);
            out.lines().next().unwrap_or("zsh").trim().to_string()
        }
        _ => "zsh".to_string(),
    }
}

/// Compares two version strings (dotted numeric).
///
/// Returns `true` if `version >= minimum`.
pub(super) fn version_gte(version: &str, minimum: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('.')
            .map(|p| {
                // Remove non-numeric suffixes like "0-rc1"
                let numeric: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
                numeric.parse().unwrap_or(0)
            })
            .collect()
    };

    let ver = parse(version);
    let min = parse(minimum);

    for i in 0..std::cmp::max(ver.len(), min.len()) {
        let v = ver.get(i).copied().unwrap_or(0);
        let m = min.get(i).copied().unwrap_or(0);
        if v > m {
            return true;
        }
        if v < m {
            return false;
        }
    }
    true // versions are equal
}

/// RAII guard that cleans up a temporary directory on drop.
pub(super) struct TempDirCleanup(pub PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        // Best effort cleanup — don't block on async in drop
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_version_gte_equal() {
        assert!(version_gte("0.36.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_greater_major() {
        assert!(version_gte("1.0.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_greater_minor() {
        assert!(version_gte("0.54.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_less() {
        assert!(!version_gte("0.35.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_with_v_prefix() {
        assert!(version_gte("v0.54.0", "0.36.0"));
    }

    #[test]
    fn test_version_gte_with_rc_suffix() {
        assert!(version_gte("0.54.0-rc1", "0.36.0"));
    }

    #[test]
    fn test_to_win_path_drive() {
        let actual = to_win_path(Path::new("/c/Users/test"));
        let expected = r"C:\Users\test";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_to_win_path_no_drive() {
        let actual = to_win_path(Path::new("/usr/bin/zsh"));
        let expected = r"\usr\bin\zsh";
        assert_eq!(actual, expected);
    }
}
