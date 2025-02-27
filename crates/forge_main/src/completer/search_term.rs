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
        // Find the start of the current word by searching backwards for whitespace
        let mut start = 0;
        for (i, c) in self.line.chars().take(self.position).enumerate().rev() {
            if c.is_whitespace() {
                start = i + 1;
                break;
            }
        }

        // Extract the term from start to cursor position
        let term_str = &self.line[start..self.position];

        // Check if the term is valid (non-empty and no whitespace)
        if term_str.is_empty() || term_str.chars().any(|c| c.is_whitespace()) {
            return None;
        }

        Some(TermResult {
            span: Span::new(start, self.position),
            term: term_str,
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
    fn test_empty_input() {
        let results = SearchTerm::test("");
        assert_debug_snapshot!(results);
    }

    #[test]
    fn test_whitespace_input() {
        let results = SearchTerm::test("   ");
        assert_debug_snapshot!(results);
    }
}