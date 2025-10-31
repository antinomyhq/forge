/// ANSI-aware utilities for parsing, trimming, and wrapping strings.
///
/// This module exposes helpers that treat SGR escape sequences as
/// zero-width while operating on visible text, preserving style.
///
/// Functions are designed to be fast and allocation-conscious while
/// remaining simple and dependable for terminal rendering.

#[derive(Clone, Copy)]
struct SgrSeg {
    is_sgr: bool,
    start: usize,
    end: usize,
}

/// Helper function to parse the sgr segments
fn parse_sgr_segments(s: &str) -> Vec<SgrSeg> {
    let b = s.as_bytes();
    let mut segs = Vec::new();
    let mut i = 0usize;
    let mut text_start = 0usize;
    while i < b.len() {
        if b[i] == 0x1B && i + 1 < b.len() && b[i + 1] as char == '[' {
            // find 'm' terminator of SGR sequence
            let mut j = i + 2;
            while j < b.len() && b[j] as char != 'm' {
                j += 1;
            }
            if j < b.len() {
                if text_start < i {
                    segs.push(SgrSeg { is_sgr: false, start: text_start, end: i });
                }
                let end = j + 1; // include 'm'
                segs.push(SgrSeg { is_sgr: true, start: i, end });
                i = end;
                text_start = i;
                continue;
            }
        }
        i += 1;
    }
    if text_start < s.len() {
        segs.push(SgrSeg { is_sgr: false, start: text_start, end: s.len() });
    }
    segs
}

/// Trim trailing visible spaces and tabs while preserving trailing SGR
/// sequences.
pub fn rtrim_visible_preserve_sgr(s: &str) -> String {
    let b = s.as_bytes();
    let segs = parse_sgr_segments(s);
    let has_sgr = segs.iter().any(|seg| seg.is_sgr);
    // Find the cut point (last non-space/tab in text segments)
    let mut cut: Option<usize> = None;
    for seg in segs.iter().rev() {
        if seg.is_sgr {
            continue;
        }
        let mut j = seg.end;
        while j > seg.start {
            let ch = b[j - 1];
            if ch == b' ' || ch == b'\t' {
                j -= 1;
            } else {
                cut = Some(j);
                break;
            }
        }
        if cut.is_some() {
            break;
        }
    }
    // If there's no visible text but there are SGR sequences,
    // preserve those SGR sequences (e.g., a lone reset code line).
    if cut.is_none() {
        if has_sgr {
            let mut only_sgr = String::new();
            for seg in &segs {
                if seg.is_sgr {
                    only_sgr.push_str(&s[seg.start..seg.end]);
                }
            }
            return only_sgr;
        } else {
            return String::new();
        }
    }
    let cut = cut.unwrap();

    // Rebuild: include all SGR segments, and text only up to the cut
    let mut out = String::with_capacity(s.len());
    for seg in segs {
        if seg.is_sgr || seg.end <= cut {
            out.push_str(&s[seg.start..seg.end]);
        } else if seg.start < cut {
            out.push_str(&s[seg.start..cut]);
        }
    }
    out
}

/// Hard-wraps text to the specified width, counting visible columns only.
///
/// ANSI SGR sequences are treated as zero-width and preserved as-is.
///
/// # Arguments
/// - `s`: Input string which may contain ANSI SGR escape sequences.
/// - `width`: Target column width for wrapping.
pub fn wrap_ansi_simple(s: &str, width: usize) -> String {
    if width == 0 {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut col = 0usize;
    for seg in parse_sgr_segments(s) {
        if seg.is_sgr {
            out.push_str(&s[seg.start..seg.end]);
            continue;
        }
        for ch in s[seg.start..seg.end].chars() {
            match ch {
                '\n' => {
                    out.push('\n');
                    col = 0;
                }
                '\r' => out.push('\r'),
                _ => {
                    if col == width {
                        out.push('\n');
                        col = 0;
                    }
                    out.push(ch);
                    col += 1;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn fixture_rtrim_preserves_sgr() {
        let fixture = "\u{1b}[31mHello\u{1b}[0m   ";
        let actual = rtrim_visible_preserve_sgr(fixture);
        let expected = "\u{1b}[31mHello\u{1b}[0m";
        assert_eq!(actual, expected);
    }

    #[test]
    fn fixture_wrap_preserves_ansi() {
        let red = "\u{1b}[31m";
        let reset = "\u{1b}[0m";
        let input = format!("{}abc{}def", red, reset);
        let actual = wrap_ansi_simple(&input, 3);
        let expected = format!("{}abc{}\ndef", red, reset);
        assert_eq!(actual, expected);
    }
}
