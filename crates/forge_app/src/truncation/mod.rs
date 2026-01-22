mod truncate_fetch;
mod truncate_mcp;
mod truncate_search;
mod truncate_shell;

pub use truncate_fetch::*;
pub use truncate_mcp::*;
pub use truncate_search::*;
pub use truncate_shell::*;

/// Truncates text content based on character limit, preserving Unicode
/// boundaries.
pub fn truncate_text(content: &str, limit: usize) -> String {
    let char_count = content.chars().count();
    if char_count <= limit {
        content.to_string()
    } else {
        content.chars().take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_truncate_text_no_truncation_needed() {
        let actual = truncate_text("short content", 100);

        assert_eq!(actual, "short content");
    }

    #[test]
    fn test_truncate_text_truncation() {
        let fixture = "a".repeat(100);

        let actual = truncate_text(&fixture, 50);

        assert_eq!(actual.len(), 50);
    }

    #[test]
    fn test_truncate_text_exact_boundary() {
        let fixture = "a".repeat(50);

        let actual = truncate_text(&fixture, 50);

        assert_eq!(actual, fixture);
    }

    #[test]
    fn test_truncate_text_unicode_safe() {
        let fixture = "Hello 世界! 🌍";

        let actual = truncate_text(fixture, 10);

        assert_eq!(actual.chars().count(), 10);
        assert_eq!(actual, "Hello 世界! ");
    }
}
