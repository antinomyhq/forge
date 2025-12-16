//! Formatting utilities for display purposes.
//!
//! Provides helpers for humanizing large numbers into readable formats.

/// Humanizes a number to a readable format with B/M/k suffixes.
///
/// # Arguments
/// * `n` - The number to humanize
///
/// # Examples
/// ```ignore
/// assert_eq!(humanize_number(1500), "1.5k");
/// assert_eq!(humanize_number(1500000), "1.5M");
/// assert_eq!(humanize_number(1500000000), "1.5B");
/// assert_eq!(humanize_number(500), "500");
/// ```
pub fn humanize_number(n: usize) -> String {
    match n {
        n if n >= 1_000_000_000 => format!("{:.1}B", n as f64 / 1_000_000_000.0),
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{:.1}k", n as f64 / 1_000.0),
        _ => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_humanize_number_billions() {
        let actual = humanize_number(1_500_000_000);
        let expected = "1.5B";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_humanize_number_millions() {
        let actual = humanize_number(2_300_000);
        let expected = "2.3M";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_humanize_number_thousands() {
        let actual = humanize_number(4_500);
        let expected = "4.5k";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_humanize_number_small() {
        let actual = humanize_number(999);
        let expected = "999";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_humanize_number_zero() {
        let actual = humanize_number(0);
        let expected = "0";
        assert_eq!(actual, expected);
    }
}
