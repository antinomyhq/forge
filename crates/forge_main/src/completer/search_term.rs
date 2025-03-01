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

    /// Get the search term from the current word at cursor position
    ///
    /// Returns the word at the cursor position.
    /// A word is defined as a sequence of non-whitespace characters.
    /// If no word is found, returns None.
    pub fn process(&self) -> Option<TermResult<'_>> {
        // Find the start of the current word (looking backwards from cursor)
        let word_start = self.line[..self.position]
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i + 1)
            .unwrap_or(0);

        // Only return a result if we have some text to work with
        if word_start < self.position {
            let term = &self.line[word_start..self.position];
            // Don't complete if the term contains spaces
            if !term.contains(' ') {
                return Some(TermResult {
                    span: Span::new(word_start, self.position),
                    term,
                });
            }
        }

        None
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
    fn test_word_based_completion() {
        let results = SearchTerm::test("abc def ghi");
        assert_debug_snapshot!(results, @r###"
        [
            TermSpec {
                input: "a[bc def ghi",
                output: Some(
                    "a",
                ),
                span_start: Some(
                    0,
                ),
                span_end: Some(
                    1,
                ),
                pos: 1,
            },
            TermSpec {
                input: "ab[c def ghi",
                output: Some(
                    "ab",
                ),
                span_start: Some(
                    0,
                ),
                span_end: Some(
                    2,
                ),
                pos: 2,
            },
            TermSpec {
                input: "abc[ def ghi",
                output: Some(
                    "abc",
                ),
                span_start: Some(
                    0,
                ),
                span_end: Some(
                    3,
                ),
                pos: 3,
            },
            TermSpec {
                input: "abc [def ghi",
                output: None,
                span_start: None,
                span_end: None,
                pos: 4,
            },
            TermSpec {
                input: "abc d[ef ghi",
                output: Some(
                    "d",
                ),
                span_start: Some(
                    4,
                ),
                span_end: Some(
                    5,
                ),
                pos: 5,
            },
            TermSpec {
                input: "abc de[f ghi",
                output: Some(
                    "de",
                ),
                span_start: Some(
                    4,
                ),
                span_end: Some(
                    6,
                ),
                pos: 6,
            },
            TermSpec {
                input: "abc def[ ghi",
                output: Some(
                    "def",
                ),
                span_start: Some(
                    4,
                ),
                span_end: Some(
                    7,
                ),
                pos: 7,
            },
            TermSpec {
                input: "abc def [ghi",
                output: None,
                span_start: None,
                span_end: None,
                pos: 8,
            },
            TermSpec {
                input: "abc def g[hi",
                output: Some(
                    "g",
                ),
                span_start: Some(
                    8,
                ),
                span_end: Some(
                    9,
                ),
                pos: 9,
            },
            TermSpec {
                input: "abc def gh[i",
                output: Some(
                    "gh",
                ),
                span_start: Some(
                    8,
                ),
                span_end: Some(
                    10,
                ),
                pos: 10,
            },
            TermSpec {
                input: "abc def ghi[",
                output: Some(
                    "ghi",
                ),
                span_start: Some(
                    8,
                ),
                span_end: Some(
                    11,
                ),
                pos: 11,
            },
        ]
        "###);
    }
}
