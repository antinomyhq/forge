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

    /// Process the current input line to determine the search term for
    /// completion
    ///
    /// For shell-like behavior:
    /// - If the line is empty or cursor is at the beginning, return an empty
    ///   term
    /// - Otherwise, find the last space before the cursor and return the text
    ///   between that space and the cursor
    /// - If no space found, return the entire line up to the cursor
    pub fn process(&self) -> Option<TermResult<'_>> {
        // Handle empty string or cursor at beginning
        if self.line.is_empty() || self.position == 0 {
            return Some(TermResult { span: Span::new(0, 0), term: "" });
        }

        // Find the last space before cursor position
        let start_pos = match self.line[..self.position].rfind(char::is_whitespace) {
            Some(pos) => pos + 1, // Start after the space
            None => 0,            // No space found, use the beginning of line
        };

        // Return the term between the last space and cursor position
        Some(TermResult {
            span: Span::new(start_pos, self.position),
            term: &self.line[start_pos..self.position],
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
    fn test_path_based_search() {
        let results = SearchTerm::test("cd /usr/local/");
        assert_debug_snapshot!(results);

        let results2 = SearchTerm::test("ls folder/sub");
        assert_debug_snapshot!(results2);

        let results3 = SearchTerm::test("");
        assert_debug_snapshot!(results3);
    }
}
