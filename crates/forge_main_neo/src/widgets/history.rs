use edtui::EditorMode;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Paragraph, StatefulWidget, Widget};

use crate::domain::{EditorStateExt, State};

/// History widget that handles autocomplete suggestion overlays
#[derive(Clone, Default)]
pub struct HistoryWidget;

impl StatefulWidget for HistoryWidget {
    type State = State;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut State,
    ) where
        Self: Sized,
    {
        // Only render autocomplete suggestions in insert mode when spotlight is not
        // visible
        if state.editor.mode != EditorMode::Insert || state.spotlight.is_visible {
            return;
        }

        let current_text = state.editor.get_text();
        if let Some(suggestion) = state.history.prefix_search(&current_text) {
            // Only show if suggestion is longer than current text
            if suggestion.len() > current_text.len() {
                let remaining_text = &suggestion[current_text.len()..];
                let cursor_pos = state.editor.cursor;

                // Split the remaining text into lines to handle wrapping properly
                let available_width = area.width.saturating_sub(cursor_pos.col as u16) as usize;
                let lines = text_to_lines(remaining_text, available_width);

                // Render each line at the correct position
                for (line_idx, line) in lines.iter().enumerate() {
                    let y_offset = cursor_pos.row as u16 + line_idx as u16;
                    if y_offset >= area.height {
                        break; // Don't render beyond area
                    }

                    let x_offset = if line_idx == 0 {
                        // First line starts from cursor position
                        cursor_pos.col as u16
                    } else {
                        // Subsequent lines start from column 0
                        0
                    };

                    let line_rect = ratatui::layout::Rect {
                        x: area.x + x_offset,
                        y: area.y + y_offset,
                        width: area.width.saturating_sub(x_offset),
                        height: 1,
                    };

                    // Ensure we don't render outside the area
                    let line_area = area.intersection(line_rect);
                    if line_area.width > 0 && line_area.height > 0 {
                        let line_paragraph = Paragraph::new(line.as_str())
                            .style(Style::default().fg(Color::DarkGray));
                        line_paragraph.render(line_area, buf);
                    }
                }
            }
        }
    }
}

/// Helper function to split text into lines based on available width
/// Handles wrapping and newlines
fn text_to_lines(text: &str, available_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    for ch in text.chars() {
        if ch == '\n' {
            lines.push(std::mem::take(&mut current_line));
        } else if current_line.len() >= available_width && !current_line.is_empty() {
            lines.push(std::mem::take(&mut current_line));
            current_line.push(ch);
        } else {
            current_line.push(ch);
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_to_lines_simple() {
        let fixture = "hello world";
        let actual = text_to_lines(fixture, 20);
        let expected = vec!["hello world".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_text_to_lines_wrapping() {
        let fixture = "hello world this is a long line";
        let actual = text_to_lines(fixture, 10);
        let expected = vec![
            "hello worl".to_string(),
            "d this is ".to_string(),
            "a long lin".to_string(),
            "e".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_text_to_lines_with_newlines() {
        let fixture = "hello\nworld\ntest";
        let actual = text_to_lines(fixture, 20);
        let expected = vec!["hello".to_string(), "world".to_string(), "test".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_text_to_lines_empty() {
        let fixture = "";
        let actual = text_to_lines(fixture, 10);
        let expected: Vec<String> = vec![];
        assert_eq!(actual, expected);
    }
}
