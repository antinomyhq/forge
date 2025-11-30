use anyhow::Result;
use rust_embed::RustEmbed;

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

        // Verify it doesn't contain comments (lines starting with #)
        for line in actual.lines() {
            let trimmed = line.trim();
            assert!(
                !trimmed.starts_with('#'),
                "Output should not contain comments, found: {}",
                line
            );
        }

        // Verify it doesn't contain empty lines
        for line in actual.lines() {
            assert!(
                !line.trim().is_empty(),
                "Output should not contain empty lines"
            );
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
