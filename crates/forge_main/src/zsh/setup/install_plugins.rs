//! Plugin and Oh My Zsh installation functions.
//!
//! Handles installation of Oh My Zsh, zsh-autosuggestions,
//! zsh-syntax-highlighting, and bashrc auto-start configuration.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use super::OMZ_INSTALL_URL;
use super::detect::zsh_custom_dir;
use super::types::BashrcConfigResult;
use super::util::{path_str, resolve_zsh_path};

/// Installs Oh My Zsh by downloading and executing the official install script.
///
/// Sets `RUNZSH=no` and `CHSH=no` to prevent the script from switching shells
/// or starting zsh automatically (we handle that ourselves).
///
/// # Errors
///
/// Returns error if the download fails or the install script exits with
/// non-zero.
pub async fn install_oh_my_zsh() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to create HTTP client")?;

    let script = client
        .get(OMZ_INSTALL_URL)
        .send()
        .await
        .context("Failed to download Oh My Zsh install script")?
        .bytes()
        .await
        .context("Failed to read Oh My Zsh install script")?;

    // Pipe the script directly to `sh -s` (like curl | sh) instead of writing
    // a temp file. The `-s` flag tells sh to read commands from stdin.
    let mut child = Command::new("sh")
        .arg("-s")
        .env("RUNZSH", "no")
        .env("CHSH", "no")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .context("Failed to spawn sh for Oh My Zsh install")?;

    // Write the script to the child's stdin, then drop to close the pipe
    if let Some(mut stdin) = child.stdin.take() {
        tokio::io::AsyncWriteExt::write_all(&mut stdin, &script)
            .await
            .context("Failed to pipe Oh My Zsh install script to sh")?;
    }

    let status = child
        .wait()
        .await
        .context("Failed to wait for Oh My Zsh install script")?;

    if !status.success() {
        bail!("Oh My Zsh installation failed. Install manually: https://ohmyz.sh/#install");
    }

    // Configure Oh My Zsh defaults in .zshrc
    configure_omz_defaults().await?;

    Ok(())
}

/// Configures Oh My Zsh defaults in `.zshrc` (theme and plugins).
async fn configure_omz_defaults() -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let zshrc_path = PathBuf::from(&home).join(".zshrc");

    if !zshrc_path.exists() {
        return Ok(());
    }

    let content = tokio::fs::read_to_string(&zshrc_path)
        .await
        .context("Failed to read .zshrc")?;

    // Create backup before modifying
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let backup_path = zshrc_path.with_file_name(format!(".zshrc.bak.{}", timestamp));
    tokio::fs::copy(&zshrc_path, &backup_path)
        .await
        .context("Failed to create .zshrc backup")?;

    let mut new_content = content.clone();

    // Set theme to robbyrussell
    let theme_re = regex::Regex::new(r#"(?m)^ZSH_THEME=.*$"#).unwrap();
    new_content = theme_re
        .replace(&new_content, r#"ZSH_THEME="robbyrussell""#)
        .to_string();

    // Set plugins
    let plugins_re = regex::Regex::new(r#"(?m)^plugins=\(.*\)$"#).unwrap();
    new_content = plugins_re
        .replace(
            &new_content,
            "plugins=(git command-not-found colored-man-pages extract z)",
        )
        .to_string();

    tokio::fs::write(&zshrc_path, &new_content)
        .await
        .context("Failed to write .zshrc")?;

    Ok(())
}

/// Installs the zsh-autosuggestions plugin via git clone into the Oh My Zsh
/// custom plugins directory.
///
/// # Errors
///
/// Returns error if git clone fails.
pub async fn install_autosuggestions() -> Result<()> {
    let dest = zsh_custom_dir()
        .context("Could not determine ZSH_CUSTOM directory")?
        .join("plugins")
        .join("zsh-autosuggestions");

    if dest.exists() {
        return Ok(());
    }

    let status = Command::new("git")
        .args([
            "clone",
            "https://github.com/zsh-users/zsh-autosuggestions.git",
            &path_str(&dest),
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to clone zsh-autosuggestions")?;

    if !status.success() {
        bail!("Failed to install zsh-autosuggestions");
    }

    Ok(())
}

/// Installs the zsh-syntax-highlighting plugin via git clone into the Oh My Zsh
/// custom plugins directory.
///
/// # Errors
///
/// Returns error if git clone fails.
pub async fn install_syntax_highlighting() -> Result<()> {
    let dest = zsh_custom_dir()
        .context("Could not determine ZSH_CUSTOM directory")?
        .join("plugins")
        .join("zsh-syntax-highlighting");

    if dest.exists() {
        return Ok(());
    }

    let status = Command::new("git")
        .args([
            "clone",
            "https://github.com/zsh-users/zsh-syntax-highlighting.git",
            &path_str(&dest),
        ])
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to clone zsh-syntax-highlighting")?;

    if !status.success() {
        bail!("Failed to install zsh-syntax-highlighting");
    }

    Ok(())
}

/// Configures `~/.bashrc` to auto-start zsh on Windows (Git Bash).
///
/// Creates necessary startup files if they don't exist, removes any previous
/// auto-start block, and appends a new one.
///
/// Returns a `BashrcConfigResult` which may contain a warning if an incomplete
/// Configures `~/.bash_profile` to auto-start zsh on Git Bash login.
///
/// Git Bash runs as a login shell and reads `~/.bash_profile` (not
/// `~/.bashrc`). The generated block sources `~/.bashrc` for user
/// customizations, then execs into zsh for interactive sessions.
///
/// Also cleans up any legacy auto-start blocks from `~/.bashrc` left by
/// previous versions of the installer.
///
/// Returns a `BashrcConfigResult` which may contain a warning if an incomplete
/// or malformed auto-start block was found and removed.
///
/// # Errors
///
/// Returns error if HOME is not set or file operations fail.
pub async fn configure_bash_profile_autostart() -> Result<BashrcConfigResult> {
    let mut result = BashrcConfigResult::default();
    let home = std::env::var("HOME").context("HOME not set")?;
    let home_path = PathBuf::from(&home);

    // Create empty sentinel files to suppress Git Bash "no such file" warnings.
    // We skip .bash_profile since we're about to write real content to it.
    for file in &[".bash_login", ".profile"] {
        let path = home_path.join(file);
        if !path.exists() {
            let _ = tokio::fs::write(&path, "").await;
        }
    }

    // --- Clean legacy auto-start blocks from ~/.bashrc ---
    let bashrc_path = home_path.join(".bashrc");
    if bashrc_path.exists()
        && let Ok(mut bashrc) = tokio::fs::read_to_string(&bashrc_path).await {
            let original = bashrc.clone();
            remove_autostart_blocks(&mut bashrc, &mut result);
            if bashrc != original {
                let _ = tokio::fs::write(&bashrc_path, &bashrc).await;
            }
        }

    // --- Write auto-start block to ~/.bash_profile ---
    let bash_profile_path = home_path.join(".bash_profile");

    let mut content = if bash_profile_path.exists() {
        tokio::fs::read_to_string(&bash_profile_path)
            .await
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Remove any previous auto-start blocks
    remove_autostart_blocks(&mut content, &mut result);

    // Resolve zsh path
    let zsh_path = resolve_zsh_path().await;

    let autostart_block =
        crate::zsh::normalize_script(include_str!("../scripts/bash_profile_autostart_block.sh"))
            .replace("{{zsh}}", &zsh_path);

    content.push_str(&autostart_block);

    tokio::fs::write(&bash_profile_path, &content)
        .await
        .context("Failed to write ~/.bash_profile")?;

    Ok(result)
}

/// End-of-block sentinel used by the new multi-line block format.
const END_MARKER: &str = "# End forge zsh setup";

/// Removes all auto-start blocks (old and new markers) from the given content.
///
/// Supports both the new `# End forge zsh setup` sentinel and the legacy
/// single-`fi` closing format (from older installer versions).
///
/// Mutates `content` in place and may set a warning on `result` if an
/// incomplete block is found.
fn remove_autostart_blocks(content: &mut String, result: &mut BashrcConfigResult) {
    loop {
        let mut found = false;
        for marker in &["# Added by zsh installer", "# Added by forge zsh setup"] {
            if let Some(start) = content.find(marker) {
                found = true;
                // Check if there's a newline before the marker (added by our block format)
                // If so, include it in the removal to prevent accumulating blank lines
                let actual_start = if start > 0 && content.as_bytes()[start - 1] == b'\n' {
                    start - 1
                } else {
                    start
                };

                // Prefer the explicit end sentinel (new format with two if/fi blocks)
                if let Some(end_offset) = content[start..].find(END_MARKER) {
                    let end = start + end_offset + END_MARKER.len();
                    // Consume trailing newline if present
                    let end = if end < content.len() && content.as_bytes()[end] == b'\n' {
                        end + 1
                    } else {
                        end
                    };
                    content.replace_range(actual_start..end, "");
                }
                // Fall back to legacy single-fi format
                else if let Some(fi_offset) = content[start..].find("\nfi\n") {
                    let end = start + fi_offset + 4; // +4 for "\nfi\n"
                    content.replace_range(actual_start..end, "");
                } else if let Some(fi_offset) = content[start..].find("\nfi") {
                    let end = start + fi_offset + 3;
                    content.replace_range(actual_start..end, "");
                } else {
                    // Incomplete block: marker found but no closing sentinel or fi
                    result.warning = Some(
                        "Found incomplete auto-start block (marker without closing sentinel). \
                         Removing incomplete block to prevent shell config corruption."
                            .to_string(),
                    );
                    content.truncate(actual_start);
                }
                break; // Process one marker at a time, then restart search
            }
        }
        if !found {
            break;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `configure_bash_profile_autostart()` with HOME set to the given
    /// temp directory, then restores the original HOME.
    async fn run_with_home(temp: &tempfile::TempDir) -> Result<BashrcConfigResult> {
        let original_home = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", temp.path()) };
        let result = configure_bash_profile_autostart().await;
        unsafe {
            match original_home {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
        result
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_writes_to_bash_profile_not_bashrc() {
        let temp = tempfile::TempDir::new().unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let bash_profile = temp.path().join(".bash_profile");
        let content = tokio::fs::read_to_string(&bash_profile).await.unwrap();

        // Should contain the auto-start block in .bash_profile
        assert!(content.contains("# Added by forge zsh setup"));
        assert!(content.contains("source \"$HOME/.bashrc\""));
        assert!(content.contains("if [ -t 0 ] && [ -x"));
        assert!(content.contains("export SHELL="));
        assert!(content.contains("exec"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_replaces_existing_block_in_bash_profile() {
        let temp = tempfile::TempDir::new().unwrap();
        let bash_profile_path = temp.path().join(".bash_profile");

        // Seed .bash_profile with an existing forge block
        let initial = include_str!("../fixtures/bashrc_with_forge_block.sh");
        tokio::fs::write(&bash_profile_path, initial).await.unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bash_profile_path).await.unwrap();

        // Original non-block content preserved
        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));
        assert!(content.contains("# More config"));
        assert!(content.contains("alias ll='ls -la'"));

        // Exactly one auto-start block
        assert_eq!(content.matches("# Added by forge zsh setup").count(), 1);
        assert_eq!(content.matches("# End forge zsh setup").count(), 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_removes_old_installer_block_from_bash_profile() {
        let temp = tempfile::TempDir::new().unwrap();
        let bash_profile_path = temp.path().join(".bash_profile");

        let initial = include_str!("../fixtures/bashrc_with_old_installer_block.sh");
        tokio::fs::write(&bash_profile_path, initial).await.unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bash_profile_path).await.unwrap();

        assert!(!content.contains("# Added by zsh installer"));
        assert!(content.contains("# Added by forge zsh setup"));
        assert_eq!(content.matches("# Added by forge zsh setup").count(), 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_cleans_legacy_block_from_bashrc() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        // Seed .bashrc with a legacy forge block (from previous installer version)
        let initial = include_str!("../fixtures/bashrc_with_forge_block.sh");
        tokio::fs::write(&bashrc_path, initial).await.unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        // .bashrc should have the forge block removed
        let bashrc = tokio::fs::read_to_string(&bashrc_path).await.unwrap();
        assert!(!bashrc.contains("# Added by forge zsh setup"));
        assert!(bashrc.contains("# My bashrc"));
        assert!(bashrc.contains("alias ll='ls -la'"));

        // .bash_profile should have the new block
        let bash_profile = tokio::fs::read_to_string(temp.path().join(".bash_profile"))
            .await
            .unwrap();
        assert!(bash_profile.contains("# Added by forge zsh setup"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_handles_incomplete_block_no_fi() {
        let temp = tempfile::TempDir::new().unwrap();
        let bash_profile_path = temp.path().join(".bash_profile");

        let initial = include_str!("../fixtures/bashrc_incomplete_block_no_fi.sh");
        tokio::fs::write(&bash_profile_path, initial).await.unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bash_profile_path).await.unwrap();

        // Original content before the incomplete block preserved
        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));

        // Exactly one complete block
        assert_eq!(content.matches("# Added by forge zsh setup").count(), 1);
        assert_eq!(content.matches("# End forge zsh setup").count(), 1);
        assert!(content.contains("if [ -t 0 ] && [ -x"));
        assert!(content.contains("export SHELL="));
        assert!(content.contains("exec"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_handles_malformed_block_missing_closing_fi() {
        let temp = tempfile::TempDir::new().unwrap();
        let bash_profile_path = temp.path().join(".bash_profile");

        // Content after the incomplete block will be lost
        let initial = include_str!("../fixtures/bashrc_malformed_block_missing_fi.sh");
        tokio::fs::write(&bash_profile_path, initial).await.unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bash_profile_path).await.unwrap();

        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));
        assert!(!content.contains("alias ll='ls -la'")); // lost after truncation

        assert!(content.contains("# Added by forge zsh setup"));
        assert_eq!(content.matches("# Added by forge zsh setup").count(), 1);
        assert_eq!(content.matches("# End forge zsh setup").count(), 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_idempotent() {
        let temp = tempfile::TempDir::new().unwrap();
        let bash_profile_path = temp.path().join(".bash_profile");

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "First run failed: {:?}", actual);
        let content_first = tokio::fs::read_to_string(&bash_profile_path).await.unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Second run failed: {:?}", actual);
        let content_second = tokio::fs::read_to_string(&bash_profile_path).await.unwrap();

        assert_eq!(content_first, content_second);
        assert_eq!(
            content_second.matches("# Added by forge zsh setup").count(),
            1
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_handles_multiple_incomplete_blocks() {
        let temp = tempfile::TempDir::new().unwrap();
        let bash_profile_path = temp.path().join(".bash_profile");

        let initial = include_str!("../fixtures/bashrc_multiple_incomplete_blocks.sh");
        tokio::fs::write(&bash_profile_path, initial).await.unwrap();

        let actual = run_with_home(&temp).await;
        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bash_profile_path).await.unwrap();

        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));
        assert_eq!(content.matches("# Added by forge zsh setup").count(), 1);
        assert_eq!(content.matches("# End forge zsh setup").count(), 1);
    }
}
