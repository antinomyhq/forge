use anyhow::{Context, Result};
use clap::CommandFactory;
use clap_complete::generate;
use clap_complete::shells::Zsh;
use rust_embed::RustEmbed;

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


