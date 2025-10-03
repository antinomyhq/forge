use std::io;

use colored::Colorize;
use forge_tracker::VERSION;

const BANNER: &str = include_str!("banner");

/// Displays the banner with version and command tips.
///
/// # Arguments
///
/// * `cli_mode` - If true, uses `:` prefix for CLI commands. If false, uses `/`
///   prefix for interactive mode.
pub fn display(cli_mode: bool) -> io::Result<()> {
    let mut banner = BANNER.to_string();

    // Always show version
    let version_label = ("Version:", VERSION);

    // Command tips with appropriate prefix based on mode
    let (new_cmd, info_cmds, model_cmd, agent_cmd, update_cmd, exit_cmd) = if cli_mode {
        (
            ":new",
            ":info, :usage, :help, :conversations",
            ":model",
            ":forge or :muse or :agent",
            ":update",
            ":exit",
        )
    } else {
        (
            "/new",
            "/info, /usage, /help, /conversations",
            "/model",
            "/forge or /muse or /agent",
            "/update",
            "/exit or <CTRL+D>",
        )
    };

    let tips = [
        ("New conversation:", new_cmd),
        ("Get started:", info_cmds),
        ("Switch model:", model_cmd),
        ("Switch agent:", agent_cmd),
        ("Update:", update_cmd),
        ("Quit:", exit_cmd),
    ];

    // Build labels array with version and tips
    let labels: Vec<(&str, &str)> = std::iter::once(version_label)
        .chain(tips.iter().copied())
        .collect();

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
