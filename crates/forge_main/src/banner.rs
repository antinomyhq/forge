use std::io;

use colored::Colorize;
use forge_tracker::VERSION;

const BANNER: &str = include_str!("banner");

/// Displays the banner with version and command tips.
///
/// # Arguments
///
/// * `cli_mode` - If true, shows CLI-relevant commands with `:` prefix. If
///   false, shows all interactive commands with `/` prefix.
///
/// # Environment Variables
///
/// * `FORGE_BANNER` - Optional custom banner text to display instead of the
///   default
pub fn display(cli_mode: bool) -> io::Result<()> {
    // Check for custom banner via environment variable
    let mut banner = std::env::var("FORGE_BANNER")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| BANNER.to_string());

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
            (
                "Warning:",
                "REPL mode is deprecated. Run `forge zsh setup` to setup zsh integration.",
            ),
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
