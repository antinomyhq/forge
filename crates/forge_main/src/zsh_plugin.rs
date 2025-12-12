use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use clap_complete::shells::Zsh;
use rust_embed::RustEmbed;

use crate::cli::Cli;

/// Embeds all shell plugin and theme files for zsh integration
#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../shell-plugin"]
#[include = "**/*.zsh"]
#[exclude = "forge.plugin.zsh"]
struct ZshPlugin;

/// Generates the complete zsh plugin by combining all embedded files
/// Strips out comments and empty lines for minimal output (except theme)
/// Includes the theme file for a complete Forge experience
pub fn generate_zsh_plugin() -> Result<String> {
    let mut output = String::new();

    // Iterate through all embedded files and combine them
    for file in ZshPlugin::iter()
        .filter(|path| path.contains("lib"))
        .flat_map(|path| ZshPlugin::get(&path).into_iter())
    {
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

    Ok(output)
}

/// Generates the ZSH theme for Forge
///
/// Returns the theme file content that can be saved to a `.zsh-theme` file
/// or sourced directly in `.zshrc`.
///
/// # Example
///
/// Save to a theme file:
/// ```bash
/// forge terminal theme zsh > ~/.oh-my-zsh/custom/themes/forge.zsh-theme
/// ```
///
/// Or source directly:
/// ```bash
/// forge terminal theme zsh >> ~/.zshrc
/// ```
pub fn generate_zsh_theme() -> Result<String> {
    let theme_file = ZshPlugin::get("forge.theme.zsh")
        .ok_or_else(|| anyhow::anyhow!("ZSH theme file not found"))?;

    let content = std::str::from_utf8(theme_file.data.as_ref())?;
    Ok(content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_zsh_plugin() {
        let output = generate_zsh_plugin().unwrap();
        insta::assert_snapshot!(output);
    }
}
