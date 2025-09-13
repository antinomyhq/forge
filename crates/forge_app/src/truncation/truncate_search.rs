use std::path::Path;

use crate::Match;
use crate::utils::format_match;

#[derive(PartialEq, Eq, Debug)]
pub enum TruncationMode {
    /// Truncation applied by number of lines
    Line,
    /// Truncation applied by number of bytes
    Byte,
    /// No truncation applied
    Full,
}

#[derive(PartialEq, Eq, Debug)]
pub struct TruncatedSearchOutput {
    pub data: Vec<String>,
    pub start: usize,
    pub total: usize,
    pub end: usize,
    pub strategy: TruncationMode,
}

impl From<Vec<String>> for TruncatedSearchOutput {
    fn from(value: Vec<String>) -> Self {
        TruncatedSearchOutput {
            start: 0,
            end: value.len(),
            total: value.len(),
            data: value,
            strategy: TruncationMode::Full,
        }
    }
}

impl TruncatedSearchOutput {
    fn truncate_by_lines(mut self, start: usize, max_lines: usize) -> Self {
        let total_lines = self.data.len();
        let is_truncated = total_lines > max_lines;
        self.data = if is_truncated {
            self.data.into_iter().skip(start).take(max_lines).collect()
        } else {
            self.data
        };

        if total_lines != self.data.len() {
            self.start = start;
            self.end = self.start.saturating_add(max_lines);
            self.strategy = TruncationMode::Line;
        }

        self
    }

    fn truncate_by_bytes(mut self, max_bytes: usize) -> Self {
        let total_lines = self.data.len();
        let input = self.data;

        let mut total_bytes = 0;
        let mut truncated = Vec::new();
        for item in input.into_iter() {
            let current_bytes = item.len();
            total_bytes += current_bytes;
            if total_bytes >= max_bytes {
                break;
            }
            truncated.push(item);
        }
        self.data = truncated;

        if self.data.len() != total_lines {
            self.end = self.start.saturating_add(self.data.len());
            self.strategy = TruncationMode::Byte;
        }

        self
    }
}

/// Truncates search output based on line limit, using search directory for
/// relative paths
pub fn truncate_search_output(
    output: &[Match],
    start_line: usize,
    max_lines: usize,
    max_bytes: usize,
    search_dir: &Path,
) -> TruncatedSearchOutput {
    let output = output
        .iter()
        .map(|v| format_match(v, search_dir))
        .collect::<Vec<_>>();

    // Apply truncation strategies
    TruncatedSearchOutput::from(output)
        .truncate_by_lines(start_line, max_lines)
        .truncate_by_bytes(max_bytes)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    impl TruncatedSearchOutput {
        pub fn with_start(mut self, start: usize) -> Self {
            self.start = start;
            self
        }

        pub fn with_end(mut self, end: usize) -> Self {
            self.end = end;
            self
        }

        pub fn with_total(mut self, total: usize) -> Self {
            self.total = total;
            self
        }

        pub fn with_strategy(mut self, strategy: TruncationMode) -> Self {
            self.strategy = strategy;
            self
        }
    }

    #[test]
    fn test_line_based_truncation() {
        let data = vec![
            "line 1".to_string(),
            "line 2".to_string(),
            "line 3".to_string(),
            "line 4".to_string(),
            "line 5".to_string(),
        ];

        let actual = TruncatedSearchOutput::from(data.clone()).truncate_by_lines(1, 3);
        let expected =
            TruncatedSearchOutput::from(data.into_iter().skip(1).take(3).collect::<Vec<_>>())
                .with_start(1)
                .with_end(4)
                .with_total(5)
                .with_strategy(TruncationMode::Line);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bytes_based_truncation() {
        // total entries = 5
        // each entry 5 bytes long
        // total size = 25 bytes
        let data = vec![
            "A".repeat(5),
            "B".repeat(5),
            "C".repeat(5),
            "D".repeat(5),
            "E".repeat(5),
        ];

        let actual = TruncatedSearchOutput::from(data.clone()).truncate_by_bytes(20);
        let expected = TruncatedSearchOutput::from(data.into_iter().take(3).collect::<Vec<_>>())
            .with_end(3)
            .with_total(5)
            .with_strategy(TruncationMode::Byte);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_both_truncation_strategies() {
        let data = vec![
            "A".repeat(900),
            "B".repeat(10),
            "C".repeat(25),
            "D".repeat(35),
            "E".repeat(45),
        ];
        let actual = TruncatedSearchOutput::from(data.clone())
            .truncate_by_lines(0, 10)
            .truncate_by_bytes(925);

        let expected = TruncatedSearchOutput::from(data.into_iter().take(2).collect::<Vec<_>>())
            .with_end(2)
            .with_total(5)
            .with_strategy(TruncationMode::Byte);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_both_truncation_strategies_with_lower_byte_limit() {
        let data = vec![
            "A".repeat(900),
            "B".repeat(10),
            "C".repeat(25),
            "D".repeat(35),
            "E".repeat(45),
        ];
        let actual = TruncatedSearchOutput::from(data.clone())
            .truncate_by_lines(0, 10)
            .truncate_by_bytes(120);
        let expected = TruncatedSearchOutput::from(vec![])
            .with_total(5)
            .with_strategy(TruncationMode::Byte);
        assert_eq!(actual, expected);
    }
}
