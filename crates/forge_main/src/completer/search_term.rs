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
        // Handle empty string case
        if self.line.is_empty() {
            return None;
        }

        // Get the substring up to the cursor position
        let line_slice = &self.line[..self.position];
        
        // Find the start of the current text segment by looking for the last whitespace
        // or the beginning of the line
        let start_position = line_slice
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        
        // Extract the current term (everything from the start position to the cursor)
        let term = &line_slice[start_position..];
        
        // Return the term and its position
        Some(TermResult {
            span: Span::new(start_position, self.position),
            term,
        })
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
