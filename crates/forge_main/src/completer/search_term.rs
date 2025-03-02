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

    /// Get the search term from the line based on '@' marker or cursor position
    ///
    /// If '@' marker is present, returns the word following it.
    /// Otherwise, returns the word at the cursor position.
    /// If no word is found, returns None.
    pub fn process(&self) -> Option<TermResult<'_>> {

        let query = &self.line[..self.position];
        let word = query
            .split_whitespace()
            .last()
            .unwrap_or("");

        if word.is_empty() {
            return None;
        }

        Some(TermResult {
            span: Span::new(self.position - word.len(), self.position),
            term: word,
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
    fn test_general_tab_completion() {
        let results = SearchTerm::test("fo");
        assert_debug_snapshot!(results);
    }

    #[test]
    fn test_command_completion() {
        let results = SearchTerm::test("/com");
        assert_debug_snapshot!(results);
    }
}
