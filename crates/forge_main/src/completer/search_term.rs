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
        let term = self
            .line
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
            .filter(|s| !s.term.contains(" "));

        term
    }
}

    /// Highlight the matching text in the suggestion
    ///
    /// This function takes the suggestion and the matched term, and highlights the matched part.
    pub fn highlight_match(suggestion: &str, matched_term: &str) -> String {
        if let Some(index) = suggestion.to_lowercase().find(&matched_term.to_lowercase()) {
            let (before, matching) = suggestion.split_at(index);
            let (matching, after) = matching.split_at(matched_term.len());
            format!("{before}\x1b[1;32m{matching}\x1b[0m{after}") // Highlight matching text in green
        } else {
            suggestion.to_string()
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
    fn test_highlight_match() {
        let suggestion = "crates/forge_app/src/tools/patch/apply.rs";
        let matched_term = "apply";
        let highlighted = SearchTerm::highlight_match(suggestion, matched_term);
        assert_eq!(
            highlighted,
            "crates/forge_app/src/tools/patch/\x1b[1;32mapply\x1b[0m.rs"
        );
    }
}
