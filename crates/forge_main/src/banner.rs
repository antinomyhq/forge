use std::io;

use colored::Colorize;

const BANNER: &str = include_str!("banner");

pub fn display() -> io::Result<()> {
    let mut banner = BANNER.to_string();

    // Define the labels as tuples of (key, value)
    let labels = [
        ("New conversation:", "/new"),
        ("Get started:", "/info, /help"),
        ("Switch mode:", "/plan or /act"),
        ("Quit:", "/exit or <CTRL+D>"),
    ];

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
