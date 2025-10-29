use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref PLAN_PATH_REGEX: Regex =
        Regex::new(r#"(?:[A-Za-z]:|/)[^\s"'`{}\[\]]*[/\\]plans[/\\][^\s"'`{}\[\]]*"#)
            .expect("Invalid regex pattern");
}

/// Extracts plan file paths from a given text string.
///
/// Searches for file paths containing `/plans/` directory using a regex
/// pattern. The pattern matches absolute paths (Unix or Windows) containing a
/// `plans` directory component.
///
/// # Arguments
/// * `text` - The text to search for plan file paths
///
/// # Returns
/// A vector of plan file path strings found in the text
pub fn extract_plan_paths(text: &str) -> Vec<String> {
    PLAN_PATH_REGEX
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_extract_plan_paths_unix_style() {
        let text = "Check /home/user/project/plans/2024-01-01-feature.md for details";
        let actual = extract_plan_paths(text);
        let expected = vec!["/home/user/project/plans/2024-01-01-feature.md".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_plan_paths_windows_style() {
        let text = "See C:\\Users\\user\\plans\\task.md";
        let actual = extract_plan_paths(text);
        let expected = vec!["C:\\Users\\user\\plans\\task.md".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_plan_paths_multiple() {
        let text = "Files: /path/plans/a.md and /other/plans/b.md";
        let actual = extract_plan_paths(text);
        let expected = vec![
            "/path/plans/a.md".to_string(),
            "/other/plans/b.md".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_plan_paths_no_match() {
        let text = "No plan files here";
        let actual = extract_plan_paths(text);
        let expected: Vec<String> = vec![];
        assert_eq!(actual, expected);
    }
}
