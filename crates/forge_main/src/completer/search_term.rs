use reedline::Span;

pub struct SearchTerm {
    line: String,
    position: usize,
}

impl SearchTerm {
    pub fn new(line: &str, position: usize) -> Self {
        if position > line.len() {
            panic!(
                "Position {position} is out of bounds: string '{line}' (length: {})",
                line.len()
            );
        }
        Self { line: line.to_string(), position }
    }

    /// Get the search term from the line based on cursor position
    ///
    /// Returns the word at the cursor position.
    /// If no word is found, returns None.
    pub fn process(&self) -> Option<TermResult<'_>> {
        // If the position is 0, there's no term to complete
        if self.position == 0 {
            return None;
        }

        // Find the start of the current word (looking backward from cursor position)
        let word_start = self.line[..self.position]
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace() || *c == '@')
            .map(|(i, c)| if c.is_whitespace() { i + 1 } else { i + 1 }) // Skip the whitespace or include @ character
            .unwrap_or(0);

        // If we're at a word boundary or the word is empty, return None
        if word_start == self.position {
            return None;
        }

        // Extract the term
        let term = &self.line[word_start..self.position];

        // Don't return terms with spaces
        if term.contains(' ') {
            return None;
        }

        Some(TermResult { span: Span::new(word_start, self.position), term })
    }
}

#[derive(Debug)]
pub struct TermResult<'a> {
    pub span: Span,
    pub term: &'a str,
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use super::SearchTerm;

    impl SearchTerm {
        fn test(line: &str) -> Vec<TermSpec> {
            (1..line.len() + 1)
                .map(|i| {
                    let input = SearchTerm::new(line, i);
                    let output = input.process();
                    let (a, b) = line.split_at(i);
                    TermSpec {
                        pos: i,
                        input: format!("{}[{}", a, b),
                        output: output.as_ref().map(|term| term.term.to_string()),
                        span_start: output.as_ref().map(|term| term.span.start),
                        span_end: output.as_ref().map(|term| term.span.end),
                    }
                })
                .collect()
        }
    }

    #[derive(Debug)]
    #[allow(dead_code)] // Used to generate test snapshots
    struct TermSpec {
        input: String,
        output: Option<String>,
        span_start: Option<usize>,
        span_end: Option<usize>,
        pos: usize,
    }

    #[test]
    fn test_marker_based_search() {
        let results = SearchTerm::test("@abc @def ghi@");
        assert_debug_snapshot!(results);
    }

    #[test]
    fn test_word_based_search() {
        let results = SearchTerm::test("abc def ghi");
        assert_debug_snapshot!("word_based_search", results);
    }

    #[test]
    fn test_mixed_search() {
        let results = SearchTerm::test("abc @def ghi");
        assert_debug_snapshot!("mixed_search", results);
    }
}
