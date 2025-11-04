use anyhow::{Result, anyhow};

/// Represents an extracted range of lines from content.
#[derive(Debug, PartialEq)]
pub struct LineRange {
    pub content: String,
    pub start: u64,
    pub end: u64,
    pub total: u64,
}

pub trait LineRangeExt {
    /// Extracts a specific range of lines from the string content.
    fn extract(&self, start: u64, end: u64) -> Result<LineRange>;
}

impl LineRangeExt for String {
    fn extract(&self, start: u64, end: u64) -> Result<LineRange> {
        self.as_str().extract(start, end)
    }
}

impl LineRangeExt for str {
    fn extract(&self, start: u64, end: u64) -> Result<LineRange> {
        // Basic validation
        if start > end {
            return Err(anyhow!(
                "Start position {start} is greater than end position {end}"
            ));
        }

        if start == 0 || end == 0 {
            return Err(anyhow!(
                "Start position {start} and end position {end} must be 1-based (inclusive)"
            ));
        }

        // Handle empty content
        if start < 2 && self.is_empty() {
            return Ok(LineRange { content: String::new(), start, end, total: 0 });
        }

        // Split into lines
        let lines: Vec<&str> = self.lines().collect();
        let total = lines.len() as u64;

        // Convert to 0-based indexing
        let start_pos = start.saturating_sub(1);
        let end_pos = end.saturating_sub(1);

        // Validate start position
        if start_pos >= total {
            return Err(anyhow!(
                "Start position {start} is beyond the content size of {total} lines"
            ));
        }

        // Cap end position at last line
        let end_pos = std::cmp::min(end_pos, total - 1);

        // Extract requested lines
        let content = if start_pos == 0 && end_pos == total - 1 {
            self.to_string() // Return full content if requesting entire range
        } else {
            lines[start_pos as usize..=end_pos as usize].join("\n")
        };

        Ok(LineRange { content, start, end, total })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_multiline_content() -> String {
        "line 1\nline 2\nline 3\nline 4\nline 5".to_string()
    }

    #[test]
    fn test_extract_start_greater_than_end() {
        let fixture = fixture_multiline_content();
        let actual = fixture.extract(5, 2);
        assert!(actual.is_err());
        assert_eq!(
            actual.unwrap_err().to_string(),
            "Start position 5 is greater than end position 2"
        );
    }

    #[test]
    fn test_extract_zero_start_position() {
        let fixture = fixture_multiline_content();
        let actual = fixture.extract(0, 3);
        assert!(actual.is_err());
        assert_eq!(
            actual.unwrap_err().to_string(),
            "Start position 0 and end position 3 must be 1-based (inclusive)"
        );
    }

    #[test]
    fn test_extract_empty_content() {
        let fixture = String::new();
        let actual = fixture.extract(1, 1).unwrap();
        let expected = LineRange { content: String::new(), start: 1, end: 1, total: 0 };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_start_beyond_content() {
        let fixture = fixture_multiline_content();
        let actual = fixture.extract(10, 15);
        assert!(actual.is_err());
        assert_eq!(
            actual.unwrap_err().to_string(),
            "Start position 10 is beyond the content size of 5 lines"
        );
    }

    #[test]
    fn test_extract_normal_range() {
        let fixture = fixture_multiline_content();
        let actual = fixture.extract(2, 4).unwrap();
        let expected = LineRange {
            content: "line 2\nline 3\nline 4".to_string(),
            start: 2,
            end: 4,
            total: 5,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_single_line() {
        let fixture = fixture_multiline_content();
        let actual = fixture.extract(3, 3).unwrap();
        let expected = LineRange { content: "line 3".to_string(), start: 3, end: 3, total: 5 };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_full_content() {
        let fixture = fixture_multiline_content();
        let actual = fixture.extract(1, 5).unwrap();
        let expected = LineRange { content: fixture.clone(), start: 1, end: 5, total: 5 };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_end_position_capping() {
        let fixture = fixture_multiline_content();
        let actual = fixture.extract(3, 100).unwrap();
        let expected = LineRange {
            content: "line 3\nline 4\nline 5".to_string(),
            start: 3,
            end: 100,
            total: 5,
        };
        assert_eq!(actual, expected);
    }
}
