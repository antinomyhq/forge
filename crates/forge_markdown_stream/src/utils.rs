//! Utility functions for the markdown renderer.

use streamdown_ansi::utils::{ansi_collapse, extract_ansi_codes, visible, visible_length};

/// Terminal theme mode (dark or light).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// Dark terminal background.
    Dark,
    /// Light terminal background.
    Light,
}

/// Detects the terminal theme mode (dark or light).
pub fn detect_theme_mode() -> ThemeMode {
    use terminal_colorsaurus::{QueryOptions, ThemeMode as ColorsaurusThemeMode, theme_mode};

    match theme_mode(QueryOptions::default()) {
        Ok(ColorsaurusThemeMode::Light) => ThemeMode::Light,
        Ok(ColorsaurusThemeMode::Dark) | Err(_) => ThemeMode::Dark,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WrapChunk {
    content: String,
    is_whitespace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WrapSegment {
    separator: String,
    word: String,
}

/// Wraps ANSI-styled text while preserving explicit whitespace between words.
///
/// Unlike the upstream streamdown wrapper, this keeps the original separator
/// string between tokens instead of reconstructing it from CJK heuristics.
pub(crate) fn wrap_text_preserving_spaces(
    text: &str,
    width: usize,
    first_prefix: &str,
    next_prefix: &str,
) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }

    let segments = wrap_segments(text);
    if segments.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_style: Vec<String> = Vec::new();

    for segment in segments {
        let separator = if current_line.is_empty() {
            ""
        } else {
            segment.separator.as_str()
        };
        let separator_width = visible_length(separator);
        let word_width = visible_length(&segment.word);
        let line_width = visible_length(&current_line);

        if current_line.is_empty() || line_width + separator_width + word_width <= width {
            current_line.push_str(separator);
            apply_style_transition(&mut current_style, separator);
            current_line.push_str(&segment.word);
            apply_style_transition(&mut current_style, &segment.word);
            continue;
        }

        push_wrapped_line(&mut lines, &current_line, first_prefix, next_prefix);

        current_line = current_style.join("");
        current_line.push_str(&segment.word);
        apply_style_transition(&mut current_style, &segment.word);
    }

    push_wrapped_line(&mut lines, &current_line, first_prefix, next_prefix);
    lines
}

/// Wraps ANSI-styled inline text without prefixes while preserving explicit
/// spaces.
pub(crate) fn simple_wrap_preserving_spaces(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    let lines = wrap_text_preserving_spaces(text, width, "", "");
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn wrap_segments(text: &str) -> Vec<WrapSegment> {
    let chunks = wrap_chunks(text);
    let mut segments = Vec::new();
    let mut separator = String::new();

    for chunk in chunks {
        if chunk.is_whitespace {
            separator.push_str(&chunk.content);
        } else {
            segments.push(WrapSegment {
                separator: std::mem::take(&mut separator),
                word: chunk.content,
            });
        }
    }

    segments
}

fn wrap_chunks(text: &str) -> Vec<WrapChunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_is_whitespace = None;
    let mut in_escape = false;
    let mut escape_buf = String::new();

    for ch in text.chars() {
        if in_escape {
            escape_buf.push(ch);
            if ch == 'm' {
                current.push_str(&escape_buf);
                escape_buf.clear();
                in_escape = false;
            }
            continue;
        }

        if ch == '\x1b' {
            in_escape = true;
            escape_buf.push(ch);
            continue;
        }

        let is_whitespace = ch.is_whitespace();
        match current_is_whitespace {
            Some(kind) if kind != is_whitespace => {
                chunks
                    .push(WrapChunk { content: std::mem::take(&mut current), is_whitespace: kind });
                current_is_whitespace = Some(is_whitespace);
            }
            None => {
                current_is_whitespace = Some(is_whitespace);
            }
            _ => {}
        }

        current.push(ch);
    }

    if !escape_buf.is_empty() {
        current.push_str(&escape_buf);
    }

    if let Some(is_whitespace) = current_is_whitespace
        && !current.is_empty()
    {
        chunks.push(WrapChunk { content: current, is_whitespace });
    }

    chunks
}

fn apply_style_transition(current_style: &mut Vec<String>, text: &str) {
    current_style.extend(extract_ansi_codes(text));
    *current_style = ansi_collapse(current_style, "");
}

fn push_wrapped_line(
    lines: &mut Vec<String>,
    current_line: &str,
    first_prefix: &str,
    next_prefix: &str,
) {
    if visible(current_line).trim().is_empty() {
        return;
    }

    let prefix = if lines.is_empty() {
        first_prefix
    } else {
        next_prefix
    };
    lines.push(format!("{prefix}{current_line}"));
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{simple_wrap_preserving_spaces, wrap_text_preserving_spaces};

    #[test]
    fn test_simple_wrap_preserving_spaces_keeps_korean_word_boundaries() {
        let fixture = "한글 공백 보존 문장";
        let actual = simple_wrap_preserving_spaces(fixture, 8);
        let expected = vec![
            "한글".to_string(),
            "공백".to_string(),
            "보존".to_string(),
            "문장".to_string(),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_preserving_spaces_keeps_multiple_spaces_on_same_line() {
        let fixture = "한글  공백 보존";
        let actual = wrap_text_preserving_spaces(fixture, 40, "", "");
        let expected = vec!["한글  공백 보존".to_string()];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_preserving_spaces_applies_prefixes_after_wrap() {
        let fixture = "한글 공백 검증";
        let actual = wrap_text_preserving_spaces(fixture, 8, "> ", "  ");
        let expected = vec![
            "> 한글".to_string(),
            "  공백".to_string(),
            "  검증".to_string(),
        ];

        assert_eq!(actual, expected);
    }
}
