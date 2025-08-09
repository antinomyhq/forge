/// Resolves and validates line ranges, ensuring they are always valid
/// and within the specified maximum size.
///
/// # Arguments
/// * `start_line` - Optional starting line position (defaults to 1)
/// * `end_line` - Optional ending line position
/// * `max_size` - The maximum allowed range size
///
/// # Returns
/// A tuple of (start_line, end_line) that is guaranteed to be valid
///
/// # Behavior
/// - If start_line is None, defaults to 1
/// - If end_line is None, defaults to start_line + max_size
/// - If end_line < start_line, swaps them to ensure valid range
/// - If range exceeds max_size, adjusts end_line to stay within limits
/// - Always ensures start_line >= 1
pub fn resolve_range(start_line: Option<u64>, end_line: Option<u64>, max_size: u64) -> (u64, u64) {
    // Handle edge case: if max_size is 0, return minimal valid range
    if max_size == 0 {
        return (1, 1);
    }

    // 1. Normalise incoming values
    let s0 = start_line.unwrap_or(1).max(1);
    let e0 = end_line.unwrap_or(s0.saturating_add(max_size.saturating_sub(1)));

    // 2. Sort them (min → start, max → end) and force start ≥ 1
    let start = s0.min(e0).max(1);
    let mut end = s0.max(e0);

    // 3. Clamp the range length to `max_size`
    end = end.min(start.saturating_add(max_size.saturating_sub(1)));

    (start, end)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_resolve_range_with_defaults() {
        let fixture = (None, None, 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 100);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_start_only() {
        let fixture = (Some(5), None, 50);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (5, 54);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_both_start_and_end() {
        let fixture = (Some(10), Some(20), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (10, 20);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_swapped_start_end() {
        let fixture = (Some(20), Some(10), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (10, 20);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_exceeding_max_size() {
        let fixture = (Some(1), Some(200), 50);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 50);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_zero_start() {
        let fixture = (Some(0), Some(10), 20);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 10);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_zero_end_swapped() {
        let fixture = (Some(5), Some(0), 20);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 5);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_exact_max_size() {
        let fixture = (Some(1), Some(10), 10);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 10);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_max_size_boundary() {
        let fixture = (Some(5), Some(16), 10);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (5, 14);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_large_numbers() {
        let fixture = (Some(1000), Some(2000), 500);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1000, 1499);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_single_line() {
        let fixture = (Some(42), Some(42), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (42, 42);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_end_only() {
        let fixture = (None, Some(50), 100);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 50);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_resolve_range_with_zero_max_size() {
        let fixture = (None, None, 0);
        let actual = resolve_range(fixture.0, fixture.1, fixture.2);
        let expected = (1, 1);
        assert_eq!(actual, expected);
    }
}
