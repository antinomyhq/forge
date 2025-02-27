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

    /// Get the search term from the line based on the cursor position.
    /// Returns the word at the cursor position.
    /// If no word is found, returns None.
    pub fn process(&self) -> Option<TermResult<'_>> {
        let line = &self.line[..self.position];
        let chars_with_indices: Vec<_> = line.char_indices().collect();

        // Find the start of the current word by searching backwards for whitespace
        let mut start_byte = 0;
        for &(byte_pos, c) in chars_with_indices.iter().rev() {
            if c.is_whitespace() {
                start_byte = byte_pos + c.len_utf8();
                break;
            }
        }

        let term_str = &line[start_byte..];

        // Check if the term is valid (non-empty and no whitespace)
        if term_str.is_empty() || term_str.contains(' ') {
            None
        } else {
            Some(TermResult { span: Span::new(start_byte, self.position), term: term_str })
        }
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
    fn test_word_based_search() {
        let results = SearchTerm::test("foo bar baz");
        assert_debug_snapshot!(results);
    }

    #[test]
    fn test_edge_cases() {
        let empty = SearchTerm::test("");
        assert_debug_snapshot!(empty);

        let whitespace = SearchTerm::test("   ");
        assert_debug_snapshot!(whitespace);

        let mid_word = SearchTerm::test("hello_world");
        assert_debug_snapshot!(mid_word);
    }
}