//! Shared marker-based shell profile setup utilities.
//!
//! Provides the core logic for inserting/updating managed configuration blocks
//! in shell profile files (`.zshrc`, PowerShell `$PROFILE`, etc.) using
//! start/end markers for idempotent updates.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// State of the forge markers in a profile file.
#[derive(Debug)]
pub enum MarkerState {
    /// No markers found
    NotFound,
    /// Valid markers with correct positions
    Valid { start: usize, end: usize },
    /// Invalid markers (incorrect order or incomplete)
    Invalid {
        start: Option<usize>,
        end: Option<usize>,
    },
}

/// Parses the file content to find and validate marker positions.
pub fn parse_markers(lines: &[String], start_marker: &str, end_marker: &str) -> MarkerState {
    let start_idx = lines.iter().position(|line| line.trim() == start_marker);
    let end_idx = lines.iter().position(|line| line.trim() == end_marker);

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start < end => MarkerState::Valid { start, end },
        (None, None) => MarkerState::NotFound,
        (start, end) => MarkerState::Invalid { start, end },
    }
}

/// Configuration for setting up a shell integration profile.
pub struct ShellSetupConfig<'a> {
    /// The start marker string (e.g., `# >>> forge initialize >>>`)
    pub start_marker: &'a str,
    /// The end marker string (e.g., `# <<< forge initialize <<<`)
    pub end_marker: &'a str,
    /// Path to the shell profile file
    pub profile_path: &'a Path,
    /// The init content to place between markers
    pub init_content: &'a str,
    /// Whether to disable nerd fonts
    pub disable_nerd_font: bool,
    /// Optional editor to configure
    pub forge_editor: Option<&'a str>,
    /// Shell-specific function to format an env var export line.
    /// Takes (key, value) and returns the full export line, e.g.:
    /// - zsh/bash: `export KEY="value"`
    /// - PowerShell: `$env:KEY = "value"`
    pub format_export: fn(&str, &str) -> String,
}

/// Result of a shell setup operation.
#[derive(Debug)]
pub struct SetupResult {
    /// Status message describing what was done
    pub message: String,
    /// Path to backup file if one was created
    pub backup_path: Option<PathBuf>,
}

/// Sets up shell integration by inserting or updating a managed block in the
/// profile file. Creates a timestamped backup before modifying existing files.
pub fn setup_shell_integration(config: &ShellSetupConfig<'_>) -> Result<SetupResult> {
    let profile_path = config.profile_path;

    // Read existing profile or start fresh
    let content = if profile_path.exists() {
        fs::read_to_string(profile_path)
            .context(format!("Failed to read {}", profile_path.display()))?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let marker_state = parse_markers(&lines, config.start_marker, config.end_marker);

    // Build the forge config block with markers
    let mut forge_config: Vec<String> = vec![config.start_marker.to_string()];
    forge_config.extend(config.init_content.lines().map(String::from));

    // Add nerd font configuration if requested
    if config.disable_nerd_font {
        forge_config.push(String::new());
        forge_config.push(
            "# Disable Nerd Fonts (set during setup - icons not displaying correctly)".to_string(),
        );
        forge_config.push(
            "# To re-enable: remove this line and install a Nerd Font from https://www.nerdfonts.com/"
                .to_string(),
        );
        forge_config.push((config.format_export)("NERD_FONT", "0"));
    }

    // Add editor configuration if requested
    if let Some(editor) = config.forge_editor {
        forge_config.push(String::new());
        forge_config.push("# Editor for editing prompts (set during setup)".to_string());
        forge_config.push(
            "# To change: update FORGE_EDITOR or remove to use $EDITOR".to_string(),
        );
        forge_config.push((config.format_export)("FORGE_EDITOR", editor));
    }

    forge_config.push(config.end_marker.to_string());

    // Add or update forge configuration block based on marker state
    let (new_content, config_action) = match marker_state {
        MarkerState::Valid { start, end } => {
            lines.splice(start..=end, forge_config.iter().cloned());
            (lines.join("\n") + "\n", "updated")
        }
        MarkerState::Invalid { start, end } => {
            let location = match (start, end) {
                (Some(s), Some(e)) => {
                    Some(format!("{}:{}-{}", profile_path.display(), s + 1, e + 1))
                }
                (Some(s), None) => Some(format!("{}:{}", profile_path.display(), s + 1)),
                (None, Some(e)) => Some(format!("{}:{}", profile_path.display(), e + 1)),
                (None, None) => None,
            };

            let mut error =
                anyhow::anyhow!("Invalid forge markers found in {}", profile_path.display());
            if let Some(loc) = location {
                error = error.context(format!("Markers found at {}", loc));
            }
            return Err(error);
        }
        MarkerState::NotFound => {
            if !lines.is_empty() && !lines[lines.len() - 1].trim().is_empty() {
                lines.push(String::new());
            }
            lines.extend(forge_config.iter().cloned());
            (lines.join("\n") + "\n", "added")
        }
    };

    // Create backup of existing profile if it exists
    let backup_path = if profile_path.exists() {
        let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
        let parent = profile_path
            .parent()
            .context("profile path has no parent directory")?;
        let filename = profile_path
            .file_name()
            .context("profile path has no filename")?;
        let filename_str = filename
            .to_str()
            .context("profile filename is not valid UTF-8")?;

        let backup = parent.join(format!("{}.bak.{}", filename_str, timestamp));
        fs::copy(profile_path, &backup)
            .context(format!("Failed to create backup at {}", backup.display()))?;
        Some(backup)
    } else {
        // Create parent directory if needed
        if let Some(parent) = profile_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .context(format!("Failed to create directory {}", parent.display()))?;
            }
        }
        None
    };

    // Write back to profile
    fs::write(profile_path, &new_content)
        .context(format!("Failed to write to {}", profile_path.display()))?;

    Ok(SetupResult {
        message: format!("forge plugins {}", config_action),
        backup_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markers_not_found() {
        let lines: Vec<String> = vec!["some line".into(), "another line".into()];
        assert!(matches!(
            parse_markers(&lines, "# >>> start >>>", "# <<< end <<<"),
            MarkerState::NotFound
        ));
    }

    #[test]
    fn test_parse_markers_valid() {
        let lines: Vec<String> = vec![
            "# >>> start >>>".into(),
            "content".into(),
            "# <<< end <<<".into(),
        ];
        assert!(matches!(
            parse_markers(&lines, "# >>> start >>>", "# <<< end <<<"),
            MarkerState::Valid { start: 0, end: 2 }
        ));
    }

    #[test]
    fn test_parse_markers_invalid_order() {
        let lines: Vec<String> = vec![
            "# <<< end <<<".into(),
            "content".into(),
            "# >>> start >>>".into(),
        ];
        assert!(matches!(
            parse_markers(&lines, "# >>> start >>>", "# <<< end <<<"),
            MarkerState::Invalid { .. }
        ));
    }

    #[test]
    fn test_parse_markers_only_start() {
        let lines: Vec<String> = vec!["# >>> start >>>".into(), "content".into()];
        assert!(matches!(
            parse_markers(&lines, "# >>> start >>>", "# <<< end <<<"),
            MarkerState::Invalid {
                start: Some(0),
                end: None
            }
        ));
    }
}
