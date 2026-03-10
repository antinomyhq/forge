//! Plugin and Oh My Zsh installation functions.
//!
//! Handles installation of Oh My Zsh, zsh-autosuggestions,
//! zsh-syntax-highlighting, and bashrc auto-start configuration.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

use super::detect::zsh_custom_dir;
use super::types::BashrcConfigResult;
use super::util::{path_str, resolve_zsh_path};
use super::OMZ_INSTALL_URL;

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
        .text()
        .await
        .context("Failed to read Oh My Zsh install script")?;

    // Write to temp file
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("omz-install.sh");
    tokio::fs::write(&script_path, &script)
        .await
        .context("Failed to write Oh My Zsh install script")?;

    // Execute the script with RUNZSH=no and CHSH=no to prevent auto-start
    // and shell changing - we handle those ourselves
    let status = Command::new("sh")
        .arg(&script_path)
        .env("RUNZSH", "no")
        .env("CHSH", "no")
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .stdin(std::process::Stdio::inherit())
        .status()
        .await
        .context("Failed to execute Oh My Zsh install script")?;

    // Clean up temp script
    let _ = tokio::fs::remove_file(&script_path).await;

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
/// block was found and removed.
///
/// # Errors
///
/// Returns error if HOME is not set or file operations fail.
pub async fn configure_bashrc_autostart() -> Result<BashrcConfigResult> {
    let mut result = BashrcConfigResult::default();
    let home = std::env::var("HOME").context("HOME not set")?;
    let home_path = PathBuf::from(&home);

    // Create empty files to suppress Git Bash warnings
    for file in &[".bash_profile", ".bash_login", ".profile"] {
        let path = home_path.join(file);
        if !path.exists() {
            let _ = tokio::fs::write(&path, "").await;
        }
    }

    let bashrc_path = home_path.join(".bashrc");

    // Read or create .bashrc
    let mut content = if bashrc_path.exists() {
        tokio::fs::read_to_string(&bashrc_path)
            .await
            .unwrap_or_default()
    } else {
        "# Created by forge zsh setup\n".to_string()
    };

    // Remove any previous auto-start blocks (from old installer or from us)
    // Loop until no more markers are found to handle multiple incomplete blocks
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

                // Find the closing "fi" line
                if let Some(fi_offset) = content[start..].find("\nfi\n") {
                    let end = start + fi_offset + 4; // +4 for "\nfi\n"
                    content.replace_range(actual_start..end, "");
                } else if let Some(fi_offset) = content[start..].find("\nfi") {
                    let end = start + fi_offset + 3;
                    content.replace_range(actual_start..end, "");
                } else {
                    // Incomplete block: marker found but no closing "fi"
                    // Remove from marker to end of file to prevent corruption
                    result.warning = Some(
                        "Found incomplete auto-start block (marker without closing 'fi'). \
                         Removing incomplete block to prevent bashrc corruption."
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

    // Resolve zsh path
    let zsh_path = resolve_zsh_path().await;

    let autostart_block =
        crate::zsh::normalize_script(include_str!("../bashrc_autostart_block.sh"))
            .replace("{{zsh}}", &zsh_path);

    content.push_str(&autostart_block);

    tokio::fs::write(&bashrc_path, &content)
        .await
        .context("Failed to write ~/.bashrc")?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn test_configure_bashrc_clean_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        // Create a clean bashrc
        let initial_content = include_str!("../fixtures/bashrc_clean.sh");
        tokio::fs::write(&bashrc_path, initial_content)
            .await
            .unwrap();

        // Set HOME to temp directory
        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = configure_bashrc_autostart().await;

        // Restore HOME
        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        // Should contain original content
        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));

        // Should contain new auto-start block
        assert!(content.contains("# Added by forge zsh setup"));
        assert!(content.contains("if [ -t 0 ] && [ -x"));
        assert!(content.contains("export SHELL="));
        assert!(content.contains("exec"));
        assert!(content.contains("fi"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_configure_bashrc_replaces_existing_block() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        // Create bashrc with existing auto-start block
        let initial_content = include_str!("../fixtures/bashrc_with_forge_block.sh");
        tokio::fs::write(&bashrc_path, initial_content)
            .await
            .unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = configure_bashrc_autostart().await;

        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        // Should contain original content
        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));
        assert!(content.contains("# More config"));
        assert!(content.contains("alias ll='ls -la'"));

        // Should have exactly one auto-start block
        let marker_count = content.matches("# Added by forge zsh setup").count();
        assert_eq!(
            marker_count, 1,
            "Should have exactly one marker, found {}",
            marker_count
        );

        // Should have exactly one fi
        let fi_count = content.matches("\nfi\n").count();
        assert_eq!(
            fi_count, 1,
            "Should have exactly one fi, found {}",
            fi_count
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_configure_bashrc_removes_old_installer_block() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        // Create bashrc with old installer block
        let initial_content = include_str!("../fixtures/bashrc_with_old_installer_block.sh");
        tokio::fs::write(&bashrc_path, initial_content)
            .await
            .unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = configure_bashrc_autostart().await;

        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        // Should NOT contain old installer marker
        assert!(!content.contains("# Added by zsh installer"));

        // Should contain new marker
        assert!(content.contains("# Added by forge zsh setup"));

        // Should have exactly one auto-start block
        let marker_count = content.matches("# Added by forge zsh setup").count();
        assert_eq!(marker_count, 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_configure_bashrc_handles_incomplete_block_no_fi() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        // Create bashrc with incomplete block (marker but no closing fi)
        let initial_content = include_str!("../fixtures/bashrc_incomplete_block_no_fi.sh");
        tokio::fs::write(&bashrc_path, initial_content)
            .await
            .unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = configure_bashrc_autostart().await;

        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        // Should contain original content before the incomplete block
        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));

        // Should have exactly one complete auto-start block
        let marker_count = content.matches("# Added by forge zsh setup").count();
        assert_eq!(
            marker_count, 1,
            "Should have exactly one marker after fixing incomplete block"
        );

        // Should have exactly one fi
        let fi_count = content.matches("\nfi\n").count();
        assert_eq!(
            fi_count, 1,
            "Should have exactly one fi after fixing incomplete block"
        );

        // The new block should be complete
        assert!(content.contains("if [ -t 0 ] && [ -x"));
        assert!(content.contains("export SHELL="));
        assert!(content.contains("exec"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_configure_bashrc_handles_malformed_block_missing_closing_fi() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        // Create bashrc with malformed block (has 'if' but closing 'fi' is missing)
        // NOTE: Content after the incomplete block will be lost since we can't
        // reliably determine where the incomplete block ends
        let initial_content = include_str!("../fixtures/bashrc_malformed_block_missing_fi.sh");
        tokio::fs::write(&bashrc_path, initial_content)
            .await
            .unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = configure_bashrc_autostart().await;

        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        // Should contain original content before the incomplete block
        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));

        // The incomplete block and everything after is removed for safety
        // This is acceptable since the file was already corrupted
        assert!(!content.contains("alias ll='ls -la'"));

        // Should have new complete block
        assert!(content.contains("# Added by forge zsh setup"));
        let marker_count = content.matches("# Added by forge zsh setup").count();
        assert_eq!(marker_count, 1);

        // Should have exactly one complete fi
        let fi_count = content.matches("\nfi\n").count();
        assert_eq!(fi_count, 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_configure_bashrc_idempotent() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        let initial_content = include_str!("../fixtures/bashrc_clean.sh");
        tokio::fs::write(&bashrc_path, initial_content)
            .await
            .unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        // Run first time
        let actual = configure_bashrc_autostart().await;
        assert!(actual.is_ok(), "First run failed: {:?}", actual);

        let content_after_first = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        // Run second time
        let actual = configure_bashrc_autostart().await;
        assert!(actual.is_ok());

        let content_after_second = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        // Both runs should produce same content (idempotent)
        assert_eq!(content_after_first, content_after_second);

        // Should have exactly one marker
        let marker_count = content_after_second
            .matches("# Added by forge zsh setup")
            .count();
        assert_eq!(marker_count, 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_configure_bashrc_handles_multiple_incomplete_blocks() {
        let temp = tempfile::TempDir::new().unwrap();
        let bashrc_path = temp.path().join(".bashrc");

        // Create bashrc with multiple incomplete blocks
        let initial_content = include_str!("../fixtures/bashrc_multiple_incomplete_blocks.sh");
        tokio::fs::write(&bashrc_path, initial_content)
            .await
            .unwrap();

        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", temp.path());
        }

        let actual = configure_bashrc_autostart().await;

        unsafe {
            if let Some(home) = original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert!(actual.is_ok(), "Should succeed: {:?}", actual);

        let content = tokio::fs::read_to_string(&bashrc_path).await.unwrap();

        // Should contain original content before incomplete blocks
        assert!(content.contains("# My bashrc"));
        assert!(content.contains("export PATH=$PATH:/usr/local/bin"));

        // Should have exactly one complete block
        let marker_count = content.matches("# Added by forge zsh setup").count();
        assert_eq!(marker_count, 1);

        // Should have exactly one fi
        let fi_count = content.matches("\nfi\n").count();
        assert_eq!(fi_count, 1);
    }
}
