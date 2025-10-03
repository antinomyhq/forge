/// Formats and prints a list of items into aligned columns for CLI output.
///
/// Takes a vector of tuples where the first element is the left column
/// and the second element is the right column. Automatically calculates
/// the maximum width of the first column and aligns all rows consistently,
/// then prints the result to stdout.
pub fn format_columns<S1: AsRef<str>, S2: AsRef<str>>(items: Vec<(S1, S2)>) {
    if items.is_empty() {
        return;
    }

    // Calculate the maximum width of the first column
    let max_width = items
        .iter()
        .map(|(col1, _)| col1.as_ref().len())
        .max()
        .unwrap_or(0);

    // Format and print each row with consistent padding
    for (col1, col2) in items {
        println!(
            "{:<width$} {}",
            col1.as_ref(),
            col2.as_ref(),
            width = max_width
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_columns_empty() {
        let items: Vec<(&str, &str)> = vec![];
        // Should not panic
        format_columns(items);
    }

    #[test]
    fn test_format_columns_with_items() {
        let items = vec![
            ("short", "Description 1"),
            ("longer-command", "Description 2"),
            ("cmd", "Description 3"),
        ];
        // Manual verification: should print with "longer-command" width alignment
        format_columns(items);
    }

    #[test]
    fn test_format_columns_with_strings() {
        let items = vec![
            ("cmd1".to_string(), "Desc 1".to_string()),
            ("cmd2".to_string(), "Desc 2".to_string()),
        ];
        format_columns(items);
    }
}
