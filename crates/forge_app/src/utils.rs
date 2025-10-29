use std::path::Path;

use regex::Regex;

use crate::{Match, MatchResult};

/// Extracts plan file paths from a given text string.
pub fn extract_plan_paths(text: &str) -> Vec<String> {
    let plan_path_regex =
        Regex::new(r#"(?:[A-Za-z]:|/)[^\s"'`{}\[\]]*[/\\]plans[/\\][^\s"'`{}\[\]]*"#)
            .expect("Invalid regex pattern");

    plan_path_regex
        .find_iter(text)
        .map(|match_obj| match_obj.as_str().to_string())
        .collect()
}

/// Formats a path for display, converting absolute paths to relative when
/// possible
///
/// If the path starts with the current working directory, returns a
/// relative path. Otherwise, returns the original absolute path.
///
/// # Arguments
/// * `path` - The path to format
/// * `cwd` - The current working directory path
///
/// # Returns
/// * A formatted path string
pub fn format_display_path(path: &Path, cwd: &Path) -> String {
    // Try to create a relative path for display if possible
    let display_path = if path.starts_with(cwd) {
        match path.strip_prefix(cwd) {
            Ok(rel_path) => rel_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    } else {
        path.display().to_string()
    };

    if display_path.is_empty() {
        ".".to_string()
    } else {
        display_path
    }
}

/// Truncates a key string for display purposes
///
/// If the key length is 20 characters or less, returns it unchanged.
/// Otherwise, shows the first 13 characters and last 4 characters with "..." in
/// between.
///
/// # Arguments
/// * `key` - The key string to truncate
///
/// # Returns
/// * A truncated version of the key for safe display
pub fn truncate_key(key: &str) -> String {
    if key.len() <= 20 {
        key.to_string()
    } else {
        format!("{}...{}", &key[..=12], &key[key.len() - 4..])
    }
}

pub fn format_match(matched: &Match, base_dir: &Path) -> String {
    match &matched.result {
        Some(MatchResult::Error(err)) => format!("Error reading {}: {}", matched.path, err),
        Some(MatchResult::Found { line_number, line }) => {
            format!(
                "{}:{}:{}",
                format_display_path(Path::new(&matched.path), base_dir),
                line_number,
                line
            )
        }
        None => format_display_path(Path::new(&matched.path), base_dir),
    }
}
