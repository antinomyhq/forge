/// Formats and prints a list of items into aligned columns for CLI output.
///
/// Takes a vector of tuples with upto 5 string-like elements.
/// Automatically calculates the maximum width for each column and aligns
/// all rows consistently, then prints the result to stdout.
pub fn format_columns<T>(items: Vec<T>)
where
    T: ColumnRow,
{
    if items.is_empty() {
        return;
    }

    // Get the number of columns and calculate max widths
    let column_count = items[0].column_count();
    let mut max_widths = vec![0; column_count];

    // Calculate maximum width for each column
    for item in &items {
        for (i, width) in item.column_widths().into_iter().enumerate() {
            max_widths[i] = max_widths[i].max(width);
        }
    }

    // Format and print each row
    for item in items {
        item.print_row(&max_widths);
    }
}

/// Trait for types that can be formatted as columns
pub trait ColumnRow {
    fn column_count(&self) -> usize;
    fn column_widths(&self) -> Vec<usize>;
    fn print_row(&self, max_widths: &[usize]);
}

// Implement for 2-column tuples (backward compatibility)
impl<S1: AsRef<str>, S2: AsRef<str>> ColumnRow for (S1, S2) {
    fn column_count(&self) -> usize {
        2
    }

    fn column_widths(&self) -> Vec<usize> {
        vec![self.0.as_ref().len(), self.1.as_ref().len()]
    }

    fn print_row(&self, max_widths: &[usize]) {
        println!(
            "{:<width1$} {}",
            self.0.as_ref(),
            self.1.as_ref(),
            width1 = max_widths[0]
        );
    }
}

// Implement for 3-column tuples
impl<S1: AsRef<str>, S2: AsRef<str>, S3: AsRef<str>> ColumnRow for (S1, S2, S3) {
    fn column_count(&self) -> usize {
        3
    }

    fn column_widths(&self) -> Vec<usize> {
        vec![
            self.0.as_ref().len(),
            self.1.as_ref().len(),
            self.2.as_ref().len(),
        ]
    }

    fn print_row(&self, max_widths: &[usize]) {
        println!(
            "{:<width1$} {:<width2$} {}",
            self.0.as_ref(),
            self.1.as_ref(),
            self.2.as_ref(),
            width1 = max_widths[0],
            width2 = max_widths[1]
        );
    }
}

// Implement for 4-column tuples
impl<S1: AsRef<str>, S2: AsRef<str>, S3: AsRef<str>, S4: AsRef<str>> ColumnRow
    for (S1, S2, S3, S4)
{
    fn column_count(&self) -> usize {
        4
    }

    fn column_widths(&self) -> Vec<usize> {
        vec![
            self.0.as_ref().len(),
            self.1.as_ref().len(),
            self.2.as_ref().len(),
            self.3.as_ref().len(),
        ]
    }

    fn print_row(&self, max_widths: &[usize]) {
        println!(
            "{:<width1$} {:<width2$} {:<width3$} {}",
            self.0.as_ref(),
            self.1.as_ref(),
            self.2.as_ref(),
            self.3.as_ref(),
            width1 = max_widths[0],
            width2 = max_widths[1],
            width3 = max_widths[2]
        );
    }
}

// Implement for 5-column tuples
impl<S1: AsRef<str>, S2: AsRef<str>, S3: AsRef<str>, S4: AsRef<str>, S5: AsRef<str>> ColumnRow
    for (S1, S2, S3, S4, S5)
{
    fn column_count(&self) -> usize {
        5
    }

    fn column_widths(&self) -> Vec<usize> {
        vec![
            self.0.as_ref().len(),
            self.1.as_ref().len(),
            self.2.as_ref().len(),
            self.3.as_ref().len(),
            self.4.as_ref().len(),
        ]
    }

    fn print_row(&self, max_widths: &[usize]) {
        println!(
            "{:<width1$} {:<width2$} {:<width3$} {:<width4$} {}",
            self.0.as_ref(),
            self.1.as_ref(),
            self.2.as_ref(),
            self.3.as_ref(),
            self.4.as_ref(),
            width1 = max_widths[0],
            width2 = max_widths[1],
            width3 = max_widths[2],
            width4 = max_widths[3]
        );
    }
}

// Implement for Vec<String> to support dynamic column counts
impl ColumnRow for Vec<String> {
    fn column_count(&self) -> usize {
        self.len()
    }

    fn column_widths(&self) -> Vec<usize> {
        self.iter().map(|s| s.len()).collect()
    }

    fn print_row(&self, max_widths: &[usize]) {
        let mut formatted = String::new();
        for (i, (col, &width)) in self.iter().zip(max_widths).enumerate() {
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
}

// Implement for Vec<&str> to support dynamic column counts
impl ColumnRow for Vec<&str> {
    fn column_count(&self) -> usize {
        self.len()
    }

    fn column_widths(&self) -> Vec<usize> {
        self.iter().map(|s| s.len()).collect()
    }

    fn print_row(&self, max_widths: &[usize]) {
        let mut formatted = String::new();
        for (i, (col, &width)) in self.iter().zip(max_widths).enumerate() {
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
}
