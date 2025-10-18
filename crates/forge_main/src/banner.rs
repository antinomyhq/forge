use std::path::Path;

use anyhow::{Context, Result};
use colored::Colorize;
use forge_tracker::VERSION;

const BANNER: &str = include_str!("banner");

/// Source for banner content
pub enum BannerSource<'a> {
    /// Use the default built-in banner
    Default,
    /// Use a custom banner from file path
    File(&'a Path),
}

/// Displays a banner with version and command tips.
///
/// # Arguments
///
/// * `source` - Banner source (default or custom file path)
/// * `cli_mode` - If true, shows CLI-relevant commands with `:` prefix. If
///   false, shows all interactive commands with `/` prefix.
pub fn display(source: BannerSource<'_>, cli_mode: bool) -> Result<()> {
    let banner_content = match source {
        BannerSource::Default => BANNER.to_string(),
        BannerSource::File(path) => std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read custom banner from '{}'", path.display()))?,
    };

    let mut banner = banner_content;

    // Always show version
    let version_label = ("Version:", VERSION);

    // Build tips based on mode
    let tips: Vec<(&str, &str)> = if cli_mode {
        // CLI mode: only show relevant commands
        vec![
            ("New conversation:", ":new"),
            ("Get started:", ":info, :conversation"),
            ("Switch model:", ":model"),
            ("Switch provider:", ":provider"),
            ("Switch agent:", ":<agent_name> e.g. :forge or :muse"),
        ]
    } else {
        // Interactive mode: show all commands
        vec![
            ("New conversation:", "/new"),
            ("Get started:", "/info, /usage, /help, /conversation"),
            ("Switch model:", "/model"),
            ("Switch agent:", "/forge or /muse or /agent"),
            ("Update:", "/update"),
            ("Quit:", "/exit or <CTRL+D>"),
        ]
    };

    // Build labels array with version and tips
    let labels: Vec<(&str, &str)> = std::iter::once(version_label).chain(tips).collect();

    // Calculate the width of the longest label key for alignment
    let max_width = labels.iter().map(|(key, _)| key.len()).max().unwrap_or(0);

    // Add all lines with right-aligned label keys and their values
    for (key, value) in &labels {
        banner.push_str(
            format!(
                "\n{}{}",
                format!("{key:>max_width$} ").dimmed(),
                value.cyan()
            )
            .as_str(),
        );
    }

    println!("{banner}\n");
    Ok(())
}

/// Displays the default banner with version and command tips.
///
/// # Arguments
///
/// * `cli_mode` - If true, shows CLI-relevant commands with `:` prefix. If
///   false, shows all interactive commands with `/` prefix.
pub fn display_default(cli_mode: bool) -> Result<()> {
    display(BannerSource::Default, cli_mode)
}
