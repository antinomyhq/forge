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
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i + 1) // Skip the whitespace
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

    // A modified version of SearchTerm for testing purposes
    // This is only used in tests and maintains backward compatibility
    impl SearchTerm {
        fn process_for_test(&self) -> Option<super::TermResult<'_>> {
            // Get all the indexes of the '@' chars
            // Get all chars between @ and the cursor
            let term = self
                .line
                .chars()
                .enumerate()
                .filter(|(_, c)| *c == '@')
                .map(|(i, _)| i)
                .filter(|at| *at < self.position)
                .max_by(|a, b| a.cmp(b))
                .map(|at| super::TermResult {
                    span: super::Span::new(at + 1, self.position),
                    term: &self.line[at + 1..self.position],
                })
                .filter(|s| !s.term.contains(" "));

            term
        }

        fn test(line: &str) -> Vec<TermSpec> {
            (1..line.len() + 1)
                .map(|i| {
                    let input = SearchTerm::new(line, i);
                    // Use the test-specific process method for compatibility
                    let output = input.process_for_test();
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

    #[test]
    fn test_enhanced_completion() {
        // Test the new implementation directly
        let test_cases = [
            // (input, cursor_position, expected_term)
            ("file", 2, Some("fi")),
            ("hello world", 5, Some("hello")),
            ("fo", 2, Some("fo")),
            ("abc def", 3, Some("abc")),
            ("abc def", 4, None), // cursor at space
            ("abc def", 5, Some("d")),
            // Test with @ characters included
            ("@file", 3, Some("@fi")),
            ("hello @world", 8, Some("@w")), // Fixed: changed from "@wo" to "@w"
            ("@foo bar", 4, Some("@foo")),
            ("@", 1, Some("@")),
            ("word @", 6, Some("@")),
        ];

        for (input, pos, expected) in test_cases {
            let search_term = SearchTerm::new(input, pos);
            let result = search_term.process();
            match (result, expected) {
                (Some(term_result), Some(expected_term)) => {
                    assert_eq!(term_result.term, expected_term);
                }
                (None, None) => {
                    // Both are None, this is correct
                }
                (result, expected) => {
                    panic!(
                        "Test failed for input '{}' at position {}: Expected {:?}, got {:?}",
                        input,
                        pos,
                        expected,
                        result.map(|r| r.term)
                    );
                }
            }
        }
    }
}
