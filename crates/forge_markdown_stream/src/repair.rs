//! Repair malformed markdown before parsing.
//!
//! This module handles common markdown issues that the parser doesn't handle well,
//! such as closing code fences on the same line as content.

use streamdown_core::ParseState;

/// Repair a line of markdown, returning one or more normalized lines.
///
/// Handles:
/// - Embedded closing fences: `}```\n` becomes `}\n` + ```` ``` ```` (only when in code block)
pub fn repair_line(line: &str, state: &ParseState) -> Vec<String> {
    // Only check for embedded closing fence when we're inside a code block
    if state.is_in_code() {
        if let Some(lines) = split_embedded_fence(line) {
            return lines;
        }
    }

    vec![line.to_string()]
}

/// Split a line if it contains an embedded closing fence at the end.
/// e.g., `}``` ` becomes Some(vec![`}`, ```` ``` ````])
fn split_embedded_fence(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim_end();

    // Check for ``` at the end
    if let Some(stripped) = trimmed.strip_suffix("```") {
        if !stripped.trim().is_empty() {
            return Some(vec![stripped.to_string(), "```".to_string()]);
        }
    }

    // Check for ~~~ at the end
    if let Some(stripped) = trimmed.strip_suffix("~~~") {
        if !stripped.trim().is_empty() {
            return Some(vec![stripped.to_string(), "~~~".to_string()]);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use streamdown_core::Code;

    fn state_outside_code() -> ParseState {
        ParseState::new()
    }

    fn state_inside_code() -> ParseState {
        let mut state = ParseState::new();
        state.enter_code_block(Code::Backtick, Some("rust".to_string()));
        state
    }

    #[test]
    fn test_normal_line_unchanged() {
        assert_eq!(repair_line("hello world", &state_outside_code()), vec!["hello world"]);
        assert_eq!(repair_line("hello world", &state_inside_code()), vec!["hello world"]);
    }

    #[test]
    fn test_valid_fence_unchanged() {
        assert_eq!(repair_line("```", &state_outside_code()), vec!["```"]);
        assert_eq!(repair_line("   ```", &state_outside_code()), vec!["   ```"]);
        assert_eq!(repair_line("```rust", &state_outside_code()), vec!["```rust"]);
    }

    #[test]
    fn test_embedded_fence_not_split_outside_code_block() {
        // Outside code block, don't split
        assert_eq!(repair_line("}```", &state_outside_code()), vec!["}```"]);
        assert_eq!(repair_line("return x;```", &state_outside_code()), vec!["return x;```"]);
    }

    #[test]
    fn test_embedded_backtick_fence_split_in_code_block() {
        // Inside code block, split embedded fences
        assert_eq!(repair_line("}```", &state_inside_code()), vec!["}", "```"]);
        assert_eq!(repair_line("     }```", &state_inside_code()), vec!["     }", "```"]);
        assert_eq!(repair_line("return x;```", &state_inside_code()), vec!["return x;", "```"]);
    }

    #[test]
    fn test_embedded_tilde_fence_split_in_code_block() {
        assert_eq!(repair_line("}~~~", &state_inside_code()), vec!["}", "~~~"]);
        assert_eq!(repair_line("return x;~~~", &state_inside_code()), vec!["return x;", "~~~"]);
    }

    #[test]
    fn test_whitespace_only_before_fence_unchanged() {
        // Just whitespace before fence is a valid fence, don't split
        assert_eq!(repair_line("   ```", &state_inside_code()), vec!["   ```"]);
        assert_eq!(repair_line("\t```", &state_inside_code()), vec!["\t```"]);
    }
}
