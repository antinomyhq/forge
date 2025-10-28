pub trait LineNumbers {
    /// Returns the text with each line numbered, starting at 1.
    fn numbered(&self) -> String {
        self.numbered_from(1)
    }

    /// Returns the text with each line numbered, starting at the given offset.
    fn numbered_from(&self, start: usize) -> String;
}

impl<T: AsRef<str>> LineNumbers for T {
    fn numbered_from(&self, start: usize) -> String {
        self.as_ref()
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{}:{}", start + i, line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numbered_default_start() {
        let text = "first line\nsecond line\nthird line";
        let expected = "1:first line\n2:second line\n3:third line";
        assert_eq!(text.numbered(), expected);
    }

    #[test]
    fn test_numbered_from_custom_start() {
        let text = "alpha\nbeta\ngamma";
        let expected = "5:alpha\n6:beta\n7:gamma";
        assert_eq!(text.numbered_from(5), expected);
    }

    #[test]
    fn test_numbered_single_line() {
        let text = "single line";
        let expected = "1:single line";
        assert_eq!(text.numbered(), expected);
    }

    #[test]
    fn test_numbered_empty_string() {
        let text = "";
        let expected = "";
        assert_eq!(text.numbered(), expected);
    }

    #[test]
    fn test_numbered_with_empty_lines() {
        let text = "line1\n\nline3";
        let expected = "1:line1\n2:\n3:line3";
        assert_eq!(text.numbered(), expected);
    }
}
