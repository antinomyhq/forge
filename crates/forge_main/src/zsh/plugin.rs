use anyhow::{Context, Result};
use clap::CommandFactory;
use clap_complete::generate;
use clap_complete::shells::Zsh;
use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;

use crate::cli::Cli;

/// Embeds shell plugin files for zsh integration
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../shell-plugin/lib"]
#[include = "**/*.zsh"]
#[exclude = "forge.plugin.zsh"]
struct ZshPluginLib;

/// Generates the complete zsh plugin by combining embedded files and clap
/// completions
pub fn generate_zsh_plugin() -> Result<String> {
    let mut output = String::new();

    // Iterate through all embedded files and combine them
    for file in ZshPluginLib::iter().flat_map(|path| ZshPluginLib::get(&path).into_iter()) {
        let content = std::str::from_utf8(file.data.as_ref())?;

        // Process other files to strip comments and empty lines
        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comment lines
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    // Generate clap completions for the CLI
    let mut cmd = Cli::command();
    let mut completions = Vec::new();
    generate(Zsh, &mut cmd, "forge", &mut completions);

    // Append completions to the output with clear separator
    let completions_str = String::from_utf8(completions)?;
    output.push_str("\n# --- Clap Completions ---\n");
    output.push_str(&completions_str);

    // Set environment variable to indicate plugin is loaded (with timestamp)
    output.push_str("\nexport _FORGE_PLUGIN_LOADED=$(date +%s)\n");

    Ok(output)
}

/// Generates the ZSH theme for Forge
pub fn generate_zsh_theme() -> Result<String> {
    let mut content = include_str!("../../../../shell-plugin/forge.theme.zsh").to_string();

    // Set environment variable to indicate theme is loaded (with timestamp)
    content.push_str("\nexport _FORGE_THEME_LOADED=$(date +%s)\n");

    Ok(content)
}

/// Runs diagnostics on the ZSH shell environment
///
/// # Errors
///
/// Returns error if the doctor script cannot be executed
pub fn run_zsh_doctor() -> Result<String> {
    // Get the embedded doctor script
    let script_content = include_str!("../../../../shell-plugin/doctor.zsh");

    // Execute the script in a zsh subprocess
    let output = std::process::Command::new("zsh")
        .arg("-c")
        .arg(script_content)
        .output()
        .context("Failed to execute zsh doctor script")?;

    // Combine stdout and stderr for complete diagnostic output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = stdout.to_string();
    if !stderr.is_empty() {
        result.push_str("\n\nErrors:\n");
        result.push_str(&stderr);
    }

    Ok(result)
}

/// Represents the state of markers in a file
enum MarkerState {
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

/// Parses the file content to find and validate marker positions
///
/// # Arguments
///
/// * `lines` - The lines of the file to parse
/// * `start_marker` - The start marker to look for
/// * `end_marker` - The end marker to look for
fn parse_markers(lines: &[String], start_marker: &str, end_marker: &str) -> MarkerState {
    let start_idx = lines.iter().position(|line| line.trim() == start_marker);
    let end_idx = lines.iter().position(|line| line.trim() == end_marker);

    match (start_idx, end_idx) {
        (Some(start), Some(end)) if start < end => MarkerState::Valid { start, end },
        (None, None) => MarkerState::NotFound,
        (start, end) => MarkerState::Invalid { start, end },
    }
}

/// Sets up ZSH integration by updating the .zshrc file using markers
///
/// # Errors
///
/// Returns error if the .zshrc file cannot be read or written
pub fn setup_zsh_integration() -> Result<String> {
    const START_MARKER: &str = "# >>> forge initialize >>>";
    const END_MARKER: &str = "# <<< forge initialize <<<";
    const FORGE_INIT_CONFIG: &str = include_str!("../../../../shell-plugin/forge.setup.zsh");

    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let zdotdir = std::env::var("ZDOTDIR").unwrap_or_else(|_| home.clone());
    let zshrc_path = PathBuf::from(&zdotdir).join(".zshrc");

    // Read existing .zshrc or create new one
    let content = if zshrc_path.exists() {
        fs::read_to_string(&zshrc_path)
            .context(format!("Failed to read {}", zshrc_path.display()))?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    // Parse markers to determine their state
    let marker_state = parse_markers(&lines, START_MARKER, END_MARKER);

    // Build the forge config block with markers
    let mut forge_config: Vec<String> = vec![START_MARKER.to_string()];
    forge_config.extend(FORGE_INIT_CONFIG.lines().map(String::from));
    forge_config.push(END_MARKER.to_string());

    // Add or update forge configuration block based on marker state
    let (new_content, config_action) = match marker_state {
        MarkerState::Valid { start, end } => {
            // Markers exist - replace content between them
            lines.splice(start..=end, forge_config.iter().cloned());
            (lines.join("\n") + "\n", "updated")
        }
        MarkerState::Invalid { start, end } => {
            let location = match (start, end) {
                (Some(s), Some(e)) => format!("{}:{}-{}", zshrc_path.display(), s + 1, e + 1),
                (Some(s), None) => format!("{}:{}", zshrc_path.display(), s + 1),
                (None, Some(e)) => format!("{}:{}", zshrc_path.display(), e + 1),
                (None, None) => unreachable!("Invalid state must have at least one marker"),
            };

            let mut error = anyhow::anyhow!("Invalid forge markers found in {}", zshrc_path.display());
            if let Some(loc) = location {
                error = error.context(format!("Markers found at {}", loc));
            }
            return Err(error);
        }
        MarkerState::NotFound => {
            // No markers - add them at the end
            // Add blank line before markers if file is not empty and doesn't end with blank line
            if !lines.is_empty() && !lines[lines.len() - 1].trim().is_empty() {
                lines.push(String::new());
            }

            lines.extend(forge_config.iter().cloned());
            (lines.join("\n") + "\n", "added")
        }
    };

    // Write back to .zshrc
    fs::write(&zshrc_path, &new_content)
        .context(format!("Failed to write to {}", zshrc_path.display()))?;

    Ok(format!("Forge configuration {}", config_action))
}
