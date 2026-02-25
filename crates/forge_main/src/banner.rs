use std::fmt;
use std::io;

use colored::Colorize;
use forge_tracker::VERSION;

const BANNER: &str = include_str!("banner");

/// Renders messages into a styled box with border characters.
struct DisplayBox {
    messages: Vec<String>,
}

impl DisplayBox {
    /// Creates a new Box with the given messages.
    fn new(messages: Vec<String>) -> Self {
        Self { messages }
    }
}

impl fmt::Display for DisplayBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let visible_len =
            |s: &str| strip_ansi_escapes::strip_str(s).chars().count();
        let width: usize = self.messages.iter().map(|s| visible_len(s)).max().unwrap_or(0) + 4;
        let top = format!("┌{}┐", "─".repeat(width.saturating_sub(2)));
        let bottom = format!("└{}┘", "─".repeat(width.saturating_sub(2)));
        let fmt_line = |s: &str| {
            let padding = width.saturating_sub(4).saturating_sub(visible_len(s));
            format!("│ {}{} │", s, " ".repeat(padding))
        };

        writeln!(f, "{}", top)?;
        for msg in &self.messages {
            writeln!(f, "{}", fmt_line(msg))?;
        }
        write!(f, "{}", bottom)
    }
}

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

    // Show deprecation warning for REPL mode after the banner
    if !cli_mode {
        display_deprecation_warning();
    }

    Ok(())
}

/// Displays a deprecation warning for REPL mode with instructions to use zsh integration.
fn display_deprecation_warning() {
    let warning = DisplayBox::new(vec![
        "REPL mode is deprecated. Please use zsh integration instead.".to_string(),
        format!(
            "{} {} {}",
            "Run".bold().yellow(),
            "forge zsh setup".bold().cyan(),
            "to get started.".bold().yellow()
        ),
    ]);
    println!("{}", warning.to_string().bold().yellow());
}
