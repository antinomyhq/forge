use super::truncate_text;

/// Result of truncating a single text value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatedText<'a> {
    /// The truncated content.
    pub content: String,
    /// Original size before truncation.
    pub original_size: usize,
    /// Full original text (for writing to temp file).
    pub full_text: &'a str,
}

/// Checks if text needs truncation and returns truncation metadata if so.
///
/// Returns `None` if text is within limit, `Some(TruncatedText)` if truncation
/// occurred.
pub fn truncate_text_if_needed<'a>(text: &'a str, limit: usize) -> Option<TruncatedText<'a>> {
    if text.len() <= limit {
        return None;
    }

    Some(TruncatedText {
        content: truncate_text(text, limit),
        original_size: text.len(),
        full_text: text,
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_no_truncation_needed() {
        let fixture = "short content";

        let actual = truncate_text_if_needed(fixture, 100);

        assert_eq!(actual, None);
    }

    #[test]
    fn test_exact_boundary() {
        let fixture = "a".repeat(50);

        let actual = truncate_text_if_needed(&fixture, 50);

        assert_eq!(actual, None);
    }

    #[test]
    fn test_truncates_long_text() {
        let fixture = "a".repeat(100);

        let actual = truncate_text_if_needed(&fixture, 50);

        let expected = TruncatedText {
            content: "a".repeat(50),
            original_size: 100,
            full_text: fixture,
        };
        assert_eq!(actual, Some(expected));
    }

    #[test]
    fn test_preserves_full_text() {
        let fixture = "hello world this is a long string";

        let actual = truncate_text_if_needed(fixture, 10);

        assert!(actual.is_some());
        let truncated = actual.unwrap();
        assert_eq!(truncated.full_text, fixture);
        assert_eq!(truncated.content, "hello worl");
        assert_eq!(truncated.original_size, 33);
    }

    #[test]
    fn test_unicode_safe() {
        let fixture = "Hello 世界! 🌍 more text here";

        let actual = truncate_text_if_needed(fixture, 10);

        assert!(actual.is_some());
        let truncated = actual.unwrap();
        assert_eq!(truncated.content.chars().count(), 10);
        assert_eq!(truncated.content, "Hello 世界! ");
    }
}
