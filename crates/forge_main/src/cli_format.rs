/// Formats and prints a list of items into aligned columns for CLI output.
///
/// Takes a vector of items that can be converted to rows (Vec<String>).
/// Automatically calculates the maximum width for each column and aligns
/// all rows consistently, then prints the result to stdout.
pub fn format_columns<T>(items: Vec<T>)
where
    T: ToRow,
{
    if items.is_empty() {
        return;
    }

    // Convert all items to rows
    let rows: Vec<Vec<String>> = items.into_iter().map(|item| item.to_row()).collect();

    if rows.is_empty() {
        return;
    }

    // Get the number of columns from the first row
    let column_count = rows[0].len();
    let mut max_widths = vec![0; column_count];

    // Calculate maximum width for each column
    for row in &rows {
        for (i, col) in row.iter().enumerate() {
            max_widths[i] = max_widths[i].max(col.len());
        }
    }

    // Format and print each row
    for row in rows {
        print_row(&row, &max_widths);
    }
}

/// Prints a single row with proper column alignment.
fn print_row(row: &[String], max_widths: &[usize]) {
    let mut formatted = String::new();
    for (i, (col, &width)) in row.iter().zip(max_widths).enumerate() {
        if i > 0 {
            formatted.push(' ');
        }
        if i == max_widths.len() - 1 {
            // Last column: no padding
            formatted.push_str(col);
        } else {
            formatted.push_str(&format!("{:<width$}", col, width = width));
        }
    }
    println!("{}", formatted);
}

/// Trait for types that can be converted to a row of strings.
pub trait ToRow {
    fn to_row(self) -> Vec<String>;
}

// Implementations for tuples with generic ToString types

impl<T1: ToString, T2: ToString> ToRow for (T1, T2) {
    fn to_row(self) -> Vec<String> {
        vec![self.0.to_string(), self.1.to_string()]
    }
}

impl<T1: ToString, T2: ToString, T3: ToString> ToRow for (T1, T2, T3) {
    fn to_row(self) -> Vec<String> {
        vec![self.0.to_string(), self.1.to_string(), self.2.to_string()]
    }
}

impl<T1: ToString, T2: ToString, T3: ToString, T4: ToString> ToRow for (T1, T2, T3, T4) {
    fn to_row(self) -> Vec<String> {
        vec![
            self.0.to_string(),
            self.1.to_string(),
            self.2.to_string(),
            self.3.to_string(),
        ]
    }
}

impl<T1: ToString, T2: ToString, T3: ToString, T4: ToString, T5: ToString> ToRow
    for (T1, T2, T3, T4, T5)
{
    fn to_row(self) -> Vec<String> {
        vec![
            self.0.to_string(),
            self.1.to_string(),
            self.2.to_string(),
            self.3.to_string(),
            self.4.to_string(),
        ]
    }
}

// Implementations for Vec types

impl<T: ToString> ToRow for Vec<T> {
    fn to_row(self) -> Vec<String> {
        self.into_iter().map(|s| s.to_string()).collect()
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

    #[test]
    fn test_format_columns_3_columns() {
        let items = vec![
            ("cmd1", "desc1", "type1"),
            ("longer-command", "description 2", "type2"),
            ("c", "d3", "t3"),
        ];
        format_columns(items);
    }

    #[test]
    fn test_format_columns_4_columns() {
        let items = vec![
            ("id", "name", "type", "status"),
            ("1", "item1", "typeA", "active"),
            ("2", "long-item-name", "typeB", "inactive"),
        ];
        format_columns(items);
    }

    #[test]
    fn test_format_columns_vec() {
        let items = vec![
            vec!["id", "name", "status"],
            vec!["1", "item1", "active"],
            vec!["2", "long-item-name", "inactive"],
        ];
        format_columns(items);
    }

    #[test]
    fn test_format_columns_vec_vec_string() {
        let items: Vec<Vec<String>> = vec![
            vec!["id".to_string(), "name".to_string(), "status".to_string()],
            vec!["1".to_string(), "item1".to_string(), "active".to_string()],
            vec![
                "2".to_string(),
                "long-item-name".to_string(),
                "inactive".to_string(),
            ],
        ];
        format_columns(items);
    }

    #[test]
    fn test_format_columns_with_numbers() {
        let items = vec![(1, "First item"), (2, "Second item"), (100, "Third item")];
        format_columns(items);
    }
}
