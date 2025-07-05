use edtui::{EditorMode, EditorState, Index2, Lines};

/// Extension trait for EditorState to provide common helper methods
pub trait EditorStateExt {
    /// Extract all text from the editor as a single string
    fn get_text(&self) -> String;

    /// Extract text from the editor as separate lines
    fn get_lines(&self) -> Vec<String>;

    /// Set the editor content to the given text and position cursor at the end
    fn set_text_with_cursor_at_end(&mut self, text: String);

    /// Set the editor content and switch to insert mode
    fn set_text_insert_mode(&mut self, text: String);

    /// Clear the editor content and reset cursor
    fn clear(&mut self);
}

impl EditorStateExt for EditorState {
    fn get_text(&self) -> String {
        self.lines
            .iter_row()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn get_lines(&self) -> Vec<String> {
        self.lines
            .iter_row()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
    }

    fn set_text_with_cursor_at_end(&mut self, text: String) {
        self.lines = Lines::from(text.clone());
        // Position cursor at the end of the text
        if let Some(last_line) = text.lines().last() {
            let line_count = text.lines().count();
            let line_len = last_line.chars().count();
            self.cursor = Index2::new(line_count.saturating_sub(1), line_len);
        } else {
            self.cursor = Index2::default();
        }
    }

    fn set_text_insert_mode(&mut self, text: String) {
        self.set_text_with_cursor_at_end(text);
        self.mode = EditorMode::Insert;
    }

    fn clear(&mut self) {
        self.lines.clear();
        self.cursor = Index2::default();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_get_text() {
        let fixture = EditorState::new(Lines::from("hello\nworld"));
        let actual = fixture.get_text();
        let expected = "hello\nworld".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_get_lines() {
        let fixture = EditorState::new(Lines::from("hello\nworld\ntest"));
        let actual = fixture.get_lines();
        let expected = vec!["hello".to_string(), "world".to_string(), "test".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_set_text_with_cursor_at_end() {
        let mut fixture = EditorState::default();
        fixture.set_text_with_cursor_at_end("hello world".to_string());

        let actual_text = fixture.get_text();
        let actual_cursor = fixture.cursor;

        let expected_text = "hello world".to_string();
        let expected_cursor = Index2::new(0, 11);

        assert_eq!(actual_text, expected_text);
        assert_eq!(actual_cursor, expected_cursor);
    }

    #[test]
    fn test_set_text_insert_mode() {
        let mut fixture = EditorState::default();
        fixture.set_text_insert_mode("test".to_string());

        let actual_text = fixture.get_text();
        let actual_mode = fixture.mode;
        let actual_cursor = fixture.cursor;

        let expected_text = "test".to_string();
        let expected_mode = EditorMode::Insert;
        let expected_cursor = Index2::new(0, 4);

        assert_eq!(actual_text, expected_text);
        assert_eq!(actual_mode, expected_mode);
        assert_eq!(actual_cursor, expected_cursor);
    }

    #[test]
    fn test_clear() {
        let mut fixture = EditorState::new(Lines::from("hello world"));
        fixture.clear();

        let actual_text = fixture.get_text();
        let actual_cursor = fixture.cursor;

        let expected_text = "".to_string();
        let expected_cursor = Index2::default();

        assert_eq!(actual_text, expected_text);
        assert_eq!(actual_cursor, expected_cursor);
    }

    #[test]
    fn test_multiline_cursor_positioning() {
        let mut fixture = EditorState::default();
        fixture.set_text_with_cursor_at_end("line1\nline2\nline3".to_string());

        let actual_cursor = fixture.cursor;
        let expected_cursor = Index2::new(2, 5); // Row 2, column 5 (end of "line3")

        assert_eq!(actual_cursor, expected_cursor);
    }
}
