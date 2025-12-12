use anyhow::Result;
use clap::CommandFactory;
use clap_complete::{generate, shells::Zsh};
use rust_embed::RustEmbed;

use crate::cli::Cli;

/// Embeds all shell plugin files for zsh integration
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../shell-plugin"]
#[include = "**/*.zsh"]
#[exclude = "forge.plugin.zsh"]
struct ZshPlugin;

/// Generates the complete zsh plugin by combining all embedded files
/// Strips out comments and empty lines for minimal output
pub fn generate_zsh_plugin() -> Result<String> {
    let mut output = String::new();

    // Iterate through all embedded files and combine them
    for file_path in ZshPlugin::iter() {
        if let Some(file) = ZshPlugin::get(&file_path) {
            let content = std::str::from_utf8(file.data.as_ref())?;

            // Process each line to strip comments and empty lines
            for line in content.lines() {
                let trimmed = line.trim();

                // Skip empty lines and comment lines
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    output.push_str(line);
                    output.push('\n');
                }
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

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_zsh_plugin() {
        let actual = generate_zsh_plugin().unwrap();

        // Verify it's not empty
        assert!(!actual.is_empty(), "Generated plugin should not be empty");

        // Verify it contains the clap completions directive
        assert!(
            actual.contains("#compdef forge"),
            "Output should contain clap completion directive"
        );

        // Verify it contains the completions separator
        assert!(
            actual.contains("# --- Clap Completions ---"),
            "Output should contain completions separator"
        );

        // Verify that the plugin content (before completions) doesn't contain comments
        // The completions section legitimately contains comments for zsh
        let lines: Vec<&str> = actual.lines().collect();
        let separator_index = lines
            .iter()
            .position(|line| line.contains("# --- Clap Completions ---"))
            .unwrap_or(lines.len());

        for line in &lines[..separator_index] {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                assert!(
                    !trimmed.starts_with('#'),
                    "Plugin content should not contain comments, found: {}",
                    line
                );
            }
        }
    }

    #[test]
    fn test_all_files_loadable() {
        let file_count = ZshPlugin::iter().count();

        // Should have at least some files embedded
        assert!(file_count > 0, "Should have embedded files");

        // Verify all files are loadable
        for file_path in ZshPlugin::iter() {
            let file = ZshPlugin::get(&file_path);
            assert!(file.is_some(), "File {} should be embedded", file_path);
        }
    }

    #[test]
    fn test_glob_pattern_includes_all_directories() {
        let files: Vec<_> = ZshPlugin::iter().collect();

        // Should have files embedded
        assert!(!files.is_empty(), "Should have embedded files");

        // Should NOT include forge.plugin.zsh (it's excluded)
        let has_plugin_file = files.iter().any(|f| f == "forge.plugin.zsh");
        assert!(!has_plugin_file, "Should exclude forge.plugin.zsh");

        // Should include files from lib/ directory
        let has_lib_files = files
            .iter()
            .any(|f| f.starts_with("lib/") && !f.contains("actions"));
        assert!(has_lib_files, "Should include lib/*.zsh files");

        // Should include files from lib/actions/ directory
        let has_action_files = files.iter().any(|f| f.starts_with("lib/actions/"));
        assert!(has_action_files, "Should include lib/actions/*.zsh files");

        // All files should end with .zsh
        for file in &files {
            assert!(
                file.ends_with(".zsh"),
                "All files should end with .zsh: {}",
                file
            );
        }
    }
}
