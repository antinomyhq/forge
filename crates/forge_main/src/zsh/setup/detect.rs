//! Dependency detection functions for the ZSH setup orchestrator.
//!
//! Detects the installation status of all dependencies: zsh, Oh My Zsh,
//! plugins, fzf, bat, fd, git, and sudo capability.

use std::path::PathBuf;

use tokio::process::Command;

use super::platform::Platform;
use super::types::*;
use super::util::{command_exists, version_gte};
use super::{BAT_MIN_VERSION, FD_MIN_VERSION, FZF_MIN_VERSION};

/// Detects whether git is available on the system.
///
/// # Returns
///
/// `true` if `git --version` succeeds, `false` otherwise.
pub async fn detect_git() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Detects the current zsh installation status.
///
/// Checks for zsh binary presence, then verifies that critical modules
/// (zle, datetime, stat) load correctly.
pub async fn detect_zsh() -> ZshStatus {
    // Find zsh binary
    let which_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };

    let output = match Command::new(which_cmd)
        .arg("zsh")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return ZshStatus::NotFound,
    };

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return ZshStatus::NotFound;
    }

    // Smoke test critical modules
    let modules_ok = Command::new("zsh")
        .args([
            "-c",
            "zmodload zsh/zle && zmodload zsh/datetime && zmodload zsh/stat",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);

    if !modules_ok {
        return ZshStatus::Broken {
            path: path.lines().next().unwrap_or(&path).to_string(),
        };
    }

    // Get version
    let version = match Command::new("zsh")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout);
            // "zsh 5.9 (x86_64-pc-linux-gnu)" -> "5.9"
            out.split_whitespace()
                .nth(1)
                .unwrap_or("unknown")
                .to_string()
        }
        _ => "unknown".to_string(),
    };

    ZshStatus::Functional {
        version,
        path: path.lines().next().unwrap_or(&path).to_string(),
    }
}

/// Detects whether Oh My Zsh is installed.
pub async fn detect_oh_my_zsh() -> OmzStatus {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return OmzStatus::NotInstalled,
    };
    let omz_path = PathBuf::from(&home).join(".oh-my-zsh");
    if omz_path.is_dir() {
        OmzStatus::Installed { path: omz_path }
    } else {
        OmzStatus::NotInstalled
    }
}

/// Returns the `$ZSH_CUSTOM` plugins directory path.
///
/// Falls back to `$HOME/.oh-my-zsh/custom` if the environment variable is not
/// set.
pub(super) fn zsh_custom_dir() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("ZSH_CUSTOM") {
        return Some(PathBuf::from(custom));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".oh-my-zsh").join("custom"))
}

/// Detects whether the zsh-autosuggestions plugin is installed.
pub async fn detect_autosuggestions() -> PluginStatus {
    match zsh_custom_dir() {
        Some(dir) if dir.join("plugins").join("zsh-autosuggestions").is_dir() => {
            PluginStatus::Installed
        }
        _ => PluginStatus::NotInstalled,
    }
}

/// Detects whether the zsh-syntax-highlighting plugin is installed.
pub async fn detect_syntax_highlighting() -> PluginStatus {
    match zsh_custom_dir() {
        Some(dir) if dir.join("plugins").join("zsh-syntax-highlighting").is_dir() => {
            PluginStatus::Installed
        }
        _ => PluginStatus::NotInstalled,
    }
}

/// Detects fzf installation and checks version against minimum requirement.
pub async fn detect_fzf() -> FzfStatus {
    // Check if fzf exists
    if !command_exists("fzf").await {
        return FzfStatus::NotFound;
    }

    let output = match Command::new("fzf")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return FzfStatus::NotFound,
    };

    let out = String::from_utf8_lossy(&output.stdout);
    // fzf --version outputs something like "0.54.0 (d4e6f0c)" or just "0.54.0"
    let version = out
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string();

    let meets_minimum = version_gte(&version, FZF_MIN_VERSION);

    FzfStatus::Found {
        version,
        meets_minimum,
    }
}

/// Detects bat installation (checks both "bat" and "batcat" on Debian/Ubuntu).
pub async fn detect_bat() -> BatStatus {
    match detect_tool_with_aliases(&["bat", "batcat"], 1, BAT_MIN_VERSION).await {
        Some((version, meets_minimum)) => BatStatus::Installed {
            version,
            meets_minimum,
        },
        None => BatStatus::NotFound,
    }
}

/// Detects fd installation (checks both "fd" and "fdfind" on Debian/Ubuntu).
pub async fn detect_fd() -> FdStatus {
    match detect_tool_with_aliases(&["fd", "fdfind"], 1, FD_MIN_VERSION).await {
        Some((version, meets_minimum)) => FdStatus::Installed {
            version,
            meets_minimum,
        },
        None => FdStatus::NotFound,
    }
}

/// Detects a tool by trying multiple command aliases, parsing the version
/// from `--version` output, and checking against a minimum version.
///
/// # Arguments
/// * `aliases` - Command names to try (e.g., `["bat", "batcat"]`)
/// * `version_word_index` - Which whitespace-delimited word in the output
///   contains the version (e.g., `"bat 0.24.0"` -> index 1)
/// * `min_version` - Minimum acceptable version string
///
/// Returns `Some((version, meets_minimum))` if any alias is found.
async fn detect_tool_with_aliases(
    aliases: &[&str],
    version_word_index: usize,
    min_version: &str,
) -> Option<(String, bool)> {
    for cmd in aliases {
        if command_exists(cmd).await
            && let Ok(output) = Command::new(cmd)
                .arg("--version")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
            && output.status.success()
        {
            let out = String::from_utf8_lossy(&output.stdout);
            let version = out
                .split_whitespace()
                .nth(version_word_index)
                .unwrap_or("unknown")
                .to_string();
            let meets_minimum = version_gte(&version, min_version);
            return Some((version, meets_minimum));
        }
    }
    None
}

/// Runs all dependency detection functions in parallel and returns aggregated
/// results.
///
/// # Returns
///
/// A `DependencyStatus` containing the status of all dependencies.
pub async fn detect_all_dependencies() -> DependencyStatus {
    let (git, zsh, oh_my_zsh, autosuggestions, syntax_highlighting, fzf, bat, fd) = tokio::join!(
        detect_git(),
        detect_zsh(),
        detect_oh_my_zsh(),
        detect_autosuggestions(),
        detect_syntax_highlighting(),
        detect_fzf(),
        detect_bat(),
        detect_fd(),
    );

    DependencyStatus {
        zsh,
        oh_my_zsh,
        autosuggestions,
        syntax_highlighting,
        fzf,
        bat,
        fd,
        git,
    }
}

/// Detects sudo capability for the current platform.
pub async fn detect_sudo(platform: Platform) -> SudoCapability {
    match platform {
        Platform::Windows | Platform::Android => SudoCapability::NoneNeeded,
        Platform::MacOS | Platform::Linux => {
            // Check if already root via `id -u`
            let is_root = Command::new("id")
                .arg("-u")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .await
                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
                .unwrap_or(false);

            if is_root {
                return SudoCapability::Root;
            }

            // Check if sudo is available
            let has_sudo = command_exists("sudo").await;

            if has_sudo {
                SudoCapability::SudoAvailable
            } else {
                SudoCapability::NoneAvailable
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_oh_my_zsh_installed() {
        let temp = tempfile::TempDir::new().unwrap();
        let omz_dir = temp.path().join(".oh-my-zsh");
        std::fs::create_dir(&omz_dir).unwrap();

        // Temporarily set HOME
        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = detect_oh_my_zsh().await;

        // Restore
        unsafe {
            if let Some(h) = original_home {
                std::env::set_var("HOME", h);
            }
        }

        assert!(matches!(actual, OmzStatus::Installed { .. }));
    }

    #[tokio::test]
    async fn test_detect_oh_my_zsh_not_installed() {
        let temp = tempfile::TempDir::new().unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = detect_oh_my_zsh().await;

        unsafe {
            if let Some(h) = original_home {
                std::env::set_var("HOME", h);
            }
        }

        assert!(matches!(actual, OmzStatus::NotInstalled));
    }

    #[tokio::test]
    async fn test_detect_autosuggestions_installed() {
        let temp = tempfile::TempDir::new().unwrap();
        let plugin_dir = temp.path().join("plugins").join("zsh-autosuggestions");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let original_custom = std::env::var("ZSH_CUSTOM").ok();
        unsafe {
            std::env::set_var("ZSH_CUSTOM", temp.path());
        }

        let actual = detect_autosuggestions().await;

        unsafe {
            if let Some(c) = original_custom {
                std::env::set_var("ZSH_CUSTOM", c);
            } else {
                std::env::remove_var("ZSH_CUSTOM");
            }
        }

        assert_eq!(actual, PluginStatus::Installed);
    }

    #[tokio::test]
    async fn test_detect_autosuggestions_not_installed() {
        let temp = tempfile::TempDir::new().unwrap();

        let original_custom = std::env::var("ZSH_CUSTOM").ok();
        unsafe {
            std::env::set_var("ZSH_CUSTOM", temp.path());
        }

        let actual = detect_autosuggestions().await;

        unsafe {
            if let Some(c) = original_custom {
                std::env::set_var("ZSH_CUSTOM", c);
            } else {
                std::env::remove_var("ZSH_CUSTOM");
            }
        }

        assert_eq!(actual, PluginStatus::NotInstalled);
    }
}
