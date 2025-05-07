use std::io;
use std::path::PathBuf;

use colored::Colorize;
use forge_domain::Environment;
use forge_tracker::VERSION;

const BANNER: &str = include_str!("banner");

#[allow(dead_code)]
pub fn display() -> io::Result<()> {
    let mut banner = BANNER.to_string();

    // Define the labels as tuples of (key, value)
    let labels: Vec<(String, String)> = vec![
        ("Version:".to_string(), VERSION.to_string()),
        ("New conversation:".to_string(), "/new".to_string()),
        ("Get started:".to_string(), "/info, /help".to_string()),
        ("Check for updates:".to_string(), "/update".to_string()),
        ("Switch mode:".to_string(), "/plan or /act".to_string()),
        ("Quit:".to_string(), "/exit or <CTRL+D>".to_string()),
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

pub fn display_with_env(env: &Environment) -> io::Result<()> {
    let mut banner = BANNER.to_string();

    // Define the labels as tuples of (key, value)
    let mut labels: Vec<(String, String)> = vec![
        ("Version:".to_string(), VERSION.to_string()),
        ("New conversation:".to_string(), "/new".to_string()),
        ("Get started:".to_string(), "/info, /help".to_string()),
        ("Check for updates:".to_string(), "/update".to_string()),
        ("Switch mode:".to_string(), "/plan or /act".to_string()),
        ("Quit:".to_string(), "/exit or <CTRL+D>".to_string()),
    ];

    // Add .env file path if available
    if let Some(dotenv_path) = &env.dotenv_path {
        // Format the path similar to the PR approach
        // Format: HH:MM:SS.mmm (hours:minutes:seconds.milliseconds)
        let now = chrono::Local::now();
        let time_str = now.format("%H:%M:%S.%3f").to_string();

        // Get relative path if possible
        // First try to replace home directory with ~
        let display_path = if let Some(home) = &env.home {
            if dotenv_path.starts_with(home) {
                // Replace home directory with ~
                let rel_path = dotenv_path.strip_prefix(home).unwrap_or(dotenv_path);
                format!("~/{}", rel_path.display())
            } else if dotenv_path.starts_with(&env.cwd) {
                // If it's in the current directory, make it relative to cwd
                let rel_path = dotenv_path.strip_prefix(&env.cwd).unwrap_or(dotenv_path);
                format!("./{}", rel_path.display())
            } else {
                // Use absolute path as fallback
                dotenv_path.display().to_string()
            }
        } else {
            // If home is not available, try to make it relative to cwd
            get_relative_path(dotenv_path, &env.cwd)
        };

        // Add the formatted message to the labels vector
        // Use a String for the key to avoid borrowing issues
        labels.insert(1, (format!("⏺ [{}]", time_str), format!("Reading {}", display_path)));
    }

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

// Helper function to get a relative path if possible
fn get_relative_path(path: &PathBuf, base: &PathBuf) -> String {
    if let Ok(relative) = path.strip_prefix(base) {
        // If we can get a relative path, use it with ./ prefix
        format!("./{}", relative.display())
    } else {
        // Otherwise use the full path
        path.display().to_string()
    }
}
