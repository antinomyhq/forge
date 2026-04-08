//! Utility functions for the markdown renderer.

use streamdown_ansi::utils::{ansi_collapse, extract_ansi_codes, visible, visible_length};
use unicode_width::UnicodeWidthChar;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum WrapAtom {
    Escape(String),
    Char(char),
}

/// Wraps ANSI-styled text while preserving explicit whitespace between words.
///
/// Unlike the upstream streamdown wrapper, this keeps the original separator
/// string between tokens instead of reconstructing it from CJK heuristics.
pub(crate) fn wrap_text_preserving_spaces(
    text: &str,
    first_width: usize,
    next_width: usize,
    first_prefix: &str,
    next_prefix: &str,
) -> Vec<String> {
    if first_width == 0 && next_width == 0 {
        return Vec::new();
    }

    let segments = wrap_segments(text);
    if segments.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_style: Vec<String> = Vec::new();
    let mut current_width = first_width;

    for segment in segments {
        let line_width = visible_length(&current_line);
        let separator = if current_line.is_empty() {
            ""
        } else {
            segment.separator.as_str()
        };
        let combined_width = visible_length(separator) + visible_length(&segment.word);

        if !current_line.is_empty() && line_width + combined_width <= current_width {
            current_line.push_str(separator);
            apply_style_transition(&mut current_style, separator);
            current_line.push_str(&segment.word);
            apply_style_transition(&mut current_style, &segment.word);
            continue;
        }

        if current_line.is_empty() && visible_length(&segment.word) <= current_width {
            current_line.push_str(&segment.word);
            apply_style_transition(&mut current_style, &segment.word);
            continue;
        }

        if !current_line.is_empty() {
            push_wrapped_line(&mut lines, &current_line, first_prefix, next_prefix);
            current_line = current_style.join("");
            current_width = next_width;
        }

        append_wrapped_word(
            &mut lines,
            &mut current_line,
            &mut current_style,
            &segment.word,
            &mut current_width,
            next_width,
            first_prefix,
            next_prefix,
        );
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

    let lines = wrap_text_preserving_spaces(text, width, width, "", "");
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn append_wrapped_word(
    lines: &mut Vec<String>,
    current_line: &mut String,
    current_style: &mut Vec<String>,
    word: &str,
    current_width: &mut usize,
    next_width: usize,
    first_prefix: &str,
    next_prefix: &str,
) {
    let mut remainder = word.to_string();

    while !remainder.is_empty() {
        let line_width = visible_length(current_line);
        let mut available = current_width.saturating_sub(line_width);

        if available == 0 {
            push_wrapped_line(lines, current_line, first_prefix, next_prefix);
            *current_line = current_style.join("");
            *current_width = next_width;
            available = (*current_width).max(1);
        }

        if visible_length(&remainder) <= available {
            current_line.push_str(&remainder);
            apply_style_transition(current_style, &remainder);
            break;
        }

        let prefix = take_prefix_fitting(&remainder, available)
            .or_else(|| take_prefix_fitting(&remainder, 1))
            .unwrap_or_else(|| remainder.clone());

        current_line.push_str(&prefix);
        apply_style_transition(current_style, &prefix);
        remainder = remainder[prefix.len()..].to_string();

        if !remainder.is_empty() {
            push_wrapped_line(lines, current_line, first_prefix, next_prefix);
            *current_line = current_style.join("");
            *current_width = next_width;
        }
    }
}

fn take_prefix_fitting(text: &str, max_width: usize) -> Option<String> {
    if text.is_empty() {
        return None;
    }

    let mut width = 0;
    let mut result = String::new();
    let mut consumed_visible = false;

    for atom in parse_atoms(text) {
        match atom {
            WrapAtom::Escape(sequence) => result.push_str(&sequence),
            WrapAtom::Char(ch) => {
                let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if consumed_visible && width + char_width > max_width {
                    break;
                }
                if !consumed_visible && char_width > max_width {
                    result.push(ch);
                    break;
                }

                result.push(ch);
                width += char_width;
                consumed_visible = true;
            }
        }
    }

    if result.is_empty() { None } else { Some(result) }
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

    for atom in parse_atoms(text) {
        match atom {
            WrapAtom::Escape(sequence) => current.push_str(&sequence),
            WrapAtom::Char(ch) => {
                let is_whitespace = ch.is_whitespace();
                match current_is_whitespace {
                    Some(kind) if kind != is_whitespace => {
                        chunks.push(WrapChunk {
                            content: std::mem::take(&mut current),
                            is_whitespace: kind,
                        });
                        current_is_whitespace = Some(is_whitespace);
                    }
                    None => {
                        current_is_whitespace = Some(is_whitespace);
                    }
                    _ => {}
                }

                current.push(ch);
            }
        }
    }

    if let Some(is_whitespace) = current_is_whitespace
        && !current.is_empty()
    {
        chunks.push(WrapChunk {
            content: current,
            is_whitespace,
        });
    }

    chunks
}

fn parse_atoms(text: &str) -> Vec<WrapAtom> {
    let mut atoms = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != 0x1b {
            let ch = text[index..].chars().next().expect("slice should start at char boundary");
            atoms.push(WrapAtom::Char(ch));
            index += ch.len_utf8();
            continue;
        }

        let end = match bytes.get(index + 1) {
            Some(b'[') => parse_csi_escape(bytes, index),
            Some(b']') => parse_osc_escape(bytes, index),
            Some(_) => (index + 2).min(bytes.len()),
            None => bytes.len(),
        };
        atoms.push(WrapAtom::Escape(text[index..end].to_string()));
        index = end;
    }

    atoms
}

fn parse_csi_escape(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index < bytes.len() {
        if (0x40..=0x7e).contains(&bytes[index]) {
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn parse_osc_escape(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index < bytes.len() {
        if bytes[index] == 0x07 {
            return index + 1;
        }
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'\\') {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
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
    use streamdown_ansi::utils::visible;

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
    fn test_simple_wrap_preserving_spaces_splits_long_tokens() {
        let fixture = "supercalifragilistic";
        let actual = simple_wrap_preserving_spaces(fixture, 5);
        let expected = vec![
            "super".to_string(),
            "calif".to_string(),
            "ragil".to_string(),
            "istic".to_string(),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_preserving_spaces_keeps_multiple_spaces_on_same_line() {
        let fixture = "한글  공백 보존";
        let actual = wrap_text_preserving_spaces(fixture, 40, 40, "", "");
        let expected = vec!["한글  공백 보존".to_string()];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_preserving_spaces_applies_prefixes_after_wrap() {
        let fixture = "한글 공백 검증";
        let actual = wrap_text_preserving_spaces(fixture, 4, 4, "> ", "  ");
        let expected = vec![
            "> 한글".to_string(),
            "  공백".to_string(),
            "  검증".to_string(),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_text_preserving_spaces_preserves_link_separator_after_osc_escape() {
        let fixture = concat!(
            "\x1b]8;;https://example.com\x1b\\",
            "link",
            "\x1b]8;;\x1b\\",
            " ",
            "\x1b[34m(https://x.co)\x1b[39m"
        );
        let actual = wrap_text_preserving_spaces(fixture, 4, 14, "", "")
            .into_iter()
            .map(|line| visible(&line))
            .collect::<Vec<_>>();
        let expected = vec!["link".to_string(), "(https://x.co)".to_string()];

        assert_eq!(actual, expected);
    }
}
