use std::fmt::Display;

pub struct NumberedContent<'a> {
    start: usize,
    raw_content: &'a str,
}

impl<'a> Display for NumberedContent<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lines: Vec<&str> = self.raw_content.lines().collect();

        if lines.is_empty() {
            return Ok(());
        }

        // Calculate the width needed for the largest line number
        let max_line_number = self.start + lines.len() - 1;
        let width = max_line_number.to_string().len();
        let start = self.start;
        let last = lines.len() - 1;
        for (i, line) in lines.into_iter().enumerate() {
            if i < last {
                writeln!(f, "{:>width$}:{}", start + i, line, width = width)?;
            } else {
                write!(f, "{:>width$}:{}", start + i, line, width = width)?;
            }
        }

        Ok(())
    }
}

pub trait LineNumbers {
    /// Returns the text with each line numbered, starting at 1.
    fn to_numbered(&self) -> NumberedContent<'_> {
        self.to_numbered_from(1)
    }

    /// Returns the text with each line numbered, starting at the given offset.
    fn to_numbered_from(&self, start: usize) -> NumberedContent<'_>;
}

impl<T: AsRef<str>> LineNumbers for T {
    fn to_numbered_from(&self, start: usize) -> NumberedContent<'_> {
        NumberedContent { start, raw_content: self.as_ref() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numbered_default_start() {
        let text = "first line\nsecond line\nthird line";
        let expected = "1:first line\n2:second line\n3:third line";
        assert_eq!(text.to_numbered().to_string(), expected);
    }

    #[test]
    fn test_numbered_from_custom_start() {
        let text = "alpha\nbeta\ngamma";
        let expected = "5:alpha\n6:beta\n7:gamma";
        assert_eq!(text.to_numbered_from(5).to_string(), expected);
    }

    #[test]
    fn test_numbered_single_line() {
        let text = "single line";
        let expected = "1:single line";
        assert_eq!(text.to_numbered().to_string(), expected);
    }

    #[test]
    fn test_numbered_empty_string() {
        let text = "";
        let expected = "";
        assert_eq!(text.to_numbered().to_string(), expected);
    }

    #[test]
    fn test_numbered_with_empty_lines() {
        let text = "line1\n\nline3";
        let expected = "1:line1\n2:\n3:line3";
        assert_eq!(text.to_numbered().to_string(), expected);
    }

    #[test]
    fn test_numbered_right_aligned_single_digit() {
        let text = "line1\nline2\nline3";
        let expected = "1:line1\n2:line2\n3:line3";
        assert_eq!(text.to_numbered().to_string(), expected);
    }

    #[test]
    fn test_numbered_right_aligned_two_digits() {
        let text = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk";
        let expected = " 1:a\n 2:b\n 3:c\n 4:d\n 5:e\n 6:f\n 7:g\n 8:h\n 9:i\n10:j\n11:k";
        assert_eq!(text.to_numbered().to_string(), expected);
    }

    #[test]
    fn test_numbered_right_aligned_three_digits() {
        let mut lines = Vec::new();
        for i in 1..=100 {
            lines.push(format!("line{}", i));
        }
        let text = lines.join("\n");
        let actual = text.to_numbered().to_string();

        // Check first line has 2 leading spaces (001 -> "  1")
        assert!(actual.starts_with("  1:line1"));
        // Check line 10 has 1 leading space (010 -> " 10")
        assert!(actual.contains("\n 10:line10\n"));
        // Check line 100 has no leading spaces (100 -> "100")
        assert!(actual.contains("\n100:line100"));
    }

    #[test]
    fn test_numbered_from_right_aligned() {
        let text = "alpha\nbeta\ngamma\ndelta";
        // Starting from 98, so max is 101 (3 digits)
        let expected = " 98:alpha\n 99:beta\n100:gamma\n101:delta";
        assert_eq!(text.to_numbered_from(98).to_string(), expected);
    }

    #[test]
    fn test_numbered_from_crosses_digit_boundary() {
        let text = "line8\nline9\nline10\nline11";
        // Starting from 8, max is 11 (2 digits)
        let expected = " 8:line8\n 9:line9\n10:line10\n11:line11";
        assert_eq!(text.to_numbered_from(8).to_string(), expected);
    }
}
