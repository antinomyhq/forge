use std::io;
use std::path::Path;

use colored::Colorize;
use dirs::home_dir;
use dotenv::dotenv;
use forge_display::title::TitleFormat;

const BANNER: &str = include_str!("banner");

pub fn display() -> io::Result<()> {
    let dotenv_path = dotenv().ok();

    // Load .env and get the path if successful
    if let Some(path) = dotenv_path {
        // Replace home path with ~ if possible
        let display_path = collapse_home_dir(&path);

        println!("{}", TitleFormat::info(format!("Reading {}", display_path)));
    }

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

fn collapse_home_dir(path: &Path) -> String {
    if let Some(home) = home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!(
                "~{}",
                std::path::MAIN_SEPARATOR.to_string() + &stripped.to_string_lossy()
            );
        }
    }
    path.display().to_string()
}
