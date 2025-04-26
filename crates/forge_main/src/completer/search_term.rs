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
        // Get all the indexes of the '@' chars
        // Get all chars between @ and the cursor
        self.line
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == '@')
            .map(|(i, _)| i)
            .filter(|at| *at < self.position)
            .max_by(|a, b| a.cmp(b))
            .map(|at| TermResult {
                span: Span::new(at + 1, self.position),
                term: &self.line[at + 1..self.position],
            })
            .filter(|s| !s.term.contains(" "))
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
                        input: format!("{a}[{b}"),
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
}
