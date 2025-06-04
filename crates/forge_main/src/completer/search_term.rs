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
        let char_indices: Vec<(usize, char)> = self.line.char_indices().collect();

        // Find cursor position in char indices
        let cursor_char_pos = char_indices
            .iter()
            .position(|(byte_idx, _)| *byte_idx >= self.position)
            .unwrap_or(char_indices.len());

        char_indices
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| *c == '@')
            .map(|(char_pos, (byte_idx, _))| (char_pos, *byte_idx))
            .filter(|(char_pos, _)| *char_pos < cursor_char_pos)
            .max_by(|a, b| a.0.cmp(&b.0))
            .and_then(|(char_pos, _)| {
                // Find the byte index after '@' character
                let start_byte_idx = if char_pos + 1 < char_indices.len() {
                    char_indices[char_pos + 1].0
                } else {
                    // If '@' is the last character, start after it
                    self.line.len()
                };
                // Find the byte index at cursor position
                let end_byte_idx = if cursor_char_pos < char_indices.len() {
                    char_indices[cursor_char_pos].0
                } else {
                    self.line.len()
                };

                let term = &self.line[start_byte_idx..end_byte_idx];
                if term.contains(" ") {
                    None
                } else {
                    Some(TermResult { span: Span::new(start_byte_idx, end_byte_idx), term })
                }
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
            let char_indices: Vec<(usize, char)> = line.char_indices().collect();

            // Test at each character boundary plus one position at the end, starting from
            // position 1
            let mut positions = Vec::new();

            // Add position for each character boundary (skip position 0 to match original
            // behavior)
            for (byte_idx, _) in &char_indices {
                if *byte_idx > 0 {
                    positions.push(*byte_idx);
                }
            }
            if !char_indices.is_empty() {
                positions.insert(0, 1); // Always start testing from position 1
            }

            // Add position at the end of string
            positions.push(line.len());

            // Remove duplicates and sort
            positions.sort();
            positions.dedup();

            positions
                .into_iter()
                .map(|pos| {
                    let input = SearchTerm::new(line, pos);
                    let output = input.process();

                    // Find the character position for display
                    let char_pos = char_indices
                        .iter()
                        .position(|(byte_idx, _)| *byte_idx >= pos)
                        .unwrap_or(char_indices.len());

                    let (a, b) = if char_pos < char_indices.len() {
                        let split_byte_pos = char_indices[char_pos].0;
                        line.split_at(split_byte_pos)
                    } else {
                        (line, "")
                    };

                    TermSpec {
                        pos,
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

    #[test]
    fn test_marker_based_search_chinese() {
        let results = SearchTerm::test("@你好 @世界 测试@");
        assert_debug_snapshot!(results);
    }

    #[test]
    fn test_marker_based_search_mixed_chinese_english() {
        let results = SearchTerm::test("@hello @世界 test@中文");
        assert_debug_snapshot!(results);
    }

    #[test]
    fn test_marker_based_search_chinese_with_spaces() {
        let results = SearchTerm::test("@中 文 @测试");
        assert_debug_snapshot!(results);
    }

    #[test]
    fn test_marker_based_search_emoji() {
        let results = SearchTerm::test("@🚀 @🌟 emoji@");
        assert_debug_snapshot!(results);
    }
}
