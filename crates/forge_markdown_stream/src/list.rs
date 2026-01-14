//! List rendering with nested indentation and bullet cycling.

use crate::inline::render_inline_content;
use crate::style::{InlineStyler, ListStyler};
use streamdown_ansi::utils::visible_length;
use streamdown_parser::ListBullet;
use streamdown_render::text::text_wrap;

/// Bullet characters for dash lists at different nesting levels.
const BULLETS_DASH: [&str; 4] = ["•", "◦", "▪", "‣"];

/// Bullet characters for asterisk lists at different nesting levels.
const BULLETS_ASTERISK: [&str; 4] = ["∗", "⁎", "✱", "✳"];

/// Bullet characters for plus lists at different nesting levels.
const BULLETS_PLUS: [&str; 4] = ["⊕", "⊙", "⊛", "⊜"];

/// List rendering state for tracking nesting and numbering.
#[derive(Default)]
pub struct ListState {
    /// Stack of (indent, is_ordered) for nested lists
    stack: Vec<(usize, bool)>,
    /// Current ordered list numbers at each level
    numbers: Vec<usize>,
    /// Whether we're in a "pending" state (saw ListEnd but might continue)
    pending_reset: bool,
}

impl ListState {
    pub fn level(&self) -> usize {
        self.stack.len()
    }

    pub fn push(&mut self, indent: usize, ordered: bool) {
        self.stack.push((indent, ordered));
        self.numbers.push(0);
    }

    pub fn pop(&mut self) {
        self.stack.pop();
        self.numbers.pop();
    }

    pub fn next_number(&mut self) -> usize {
        if let Some(n) = self.numbers.last_mut() {
            *n += 1;
            *n
        } else {
            1
        }
    }

    pub fn adjust_for_indent(&mut self, indent: usize, ordered: bool) {
        // Pop levels that are deeper than current
        while let Some((stack_indent, _)) = self.stack.last() {
            if *stack_indent > indent {
                self.pop();
            } else {
                break;
            }
        }

        // Check if we need to push a new level
        let need_push = self.stack.last().map(|(i, _)| indent > *i).unwrap_or(true);
        if need_push {
            self.push(indent, ordered);
        }
    }

    pub fn reset(&mut self) {
        self.stack.clear();
        self.numbers.clear();
        self.pending_reset = false;
    }

    /// Mark list as pending reset (saw ListEnd, but might continue with more items)
    pub fn mark_pending_reset(&mut self) {
        self.pending_reset = true;
    }

    /// Resume list if it was pending reset (new list item arrived)
    fn resume_if_pending(&mut self) {
        self.pending_reset = false;
    }
}

/// Render a list item.
pub fn render_list_item<S: InlineStyler + ListStyler>(
    indent: usize,
    bullet: &ListBullet,
    content: &str,
    width: usize,
    margin: &str,
    styler: &S,
    list_state: &mut ListState,
) -> Vec<String> {
    // Resume list if it was pending reset (continues after empty line)
    list_state.resume_if_pending();

    // Adjust list state for current indent
    let ordered = matches!(bullet, ListBullet::Ordered(_));
    list_state.adjust_for_indent(indent, ordered);

    let level = list_state.level().saturating_sub(1);

    // Calculate marker - use our own counter for ordered lists to work around
    // the parser bug that normalizes all numbers to 1
    let marker = match bullet {
        ListBullet::Ordered(_) => {
            let num = list_state.next_number();
            format!("{}.", num)
        }
        ListBullet::PlusExpand => "⊞".to_string(),
        ListBullet::Dash => BULLETS_DASH[level % BULLETS_DASH.len()].to_string(),
        ListBullet::Asterisk => BULLETS_ASTERISK[level % BULLETS_ASTERISK.len()].to_string(),
        ListBullet::Plus => BULLETS_PLUS[level % BULLETS_PLUS.len()].to_string(),
    };

    // Calculate indentation
    let indent_spaces = indent * 2;
    let marker_width = visible_length(&marker);
    let content_indent = indent_spaces + marker_width + 1;

    // Color the marker based on bullet type
    let colored_marker = match bullet {
        ListBullet::Ordered(_) => styler.number(&marker),
        ListBullet::Dash => styler.bullet_dash(&marker),
        ListBullet::Asterisk => styler.bullet_asterisk(&marker),
        ListBullet::Plus => styler.bullet_plus(&marker),
        ListBullet::PlusExpand => styler.bullet_plus_expand(&marker),
    };

    // Parse and render inline content
    let rendered_content = render_inline_content(content, styler);

    // Build prefixes
    let first_prefix = format!("{}{}{} ", margin, " ".repeat(indent_spaces), colored_marker);
    let next_prefix = format!("{}{}", margin, " ".repeat(content_indent));

    // Wrap the content
    let wrapped = text_wrap(&rendered_content, width, 0, &first_prefix, &next_prefix, false, true);

    if wrapped.is_empty() {
        vec![first_prefix]
    } else {
        wrapped.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::TagStyler;

    fn render(indent: usize, bullet: ListBullet, content: &str) -> String {
        let mut state = ListState::default();
        render_list_item(indent, &bullet, content, 80, "  ", &TagStyler, &mut state).join("\n")
    }

    fn render_with_state(
        indent: usize,
        bullet: ListBullet,
        content: &str,
        state: &mut ListState,
    ) -> String {
        render_list_item(indent, &bullet, content, 80, "  ", &TagStyler, state).join("\n")
    }

    fn render_with_width(indent: usize, bullet: ListBullet, content: &str, width: usize) -> String {
        let mut state = ListState::default();
        render_list_item(indent, &bullet, content, width, "  ", &TagStyler, &mut state).join("\n")
    }

    #[test]
    fn test_unordered_dash() {
        insta::assert_snapshot!(render(0, ListBullet::Dash, "Item one"), @"  <dash>•</dash> Item one");
    }

    #[test]
    fn test_unordered_asterisk() {
        insta::assert_snapshot!(render(0, ListBullet::Asterisk, "Item two"), @"  <asterisk>∗</asterisk> Item two");
    }

    #[test]
    fn test_unordered_plus() {
        insta::assert_snapshot!(render(0, ListBullet::Plus, "Item three"), @"  <plus>⊕</plus> Item three");
    }

    #[test]
    fn test_ordered_item() {
        insta::assert_snapshot!(render(0, ListBullet::Ordered(1), "First item"), @"  <num>1.</num> First item");
    }

    #[test]
    fn test_ordered_sequential() {
        let mut state = ListState::default();
        let first = render_with_state(0, ListBullet::Ordered(1), "First", &mut state);
        let second = render_with_state(0, ListBullet::Ordered(1), "Second", &mut state);
        let third = render_with_state(0, ListBullet::Ordered(1), "Third", &mut state);

        insta::assert_snapshot!(first, @"  <num>1.</num> First");
        insta::assert_snapshot!(second, @"  <num>2.</num> Second");
        insta::assert_snapshot!(third, @"  <num>3.</num> Third");
    }

    #[test]
    fn test_plus_expand() {
        insta::assert_snapshot!(render(0, ListBullet::PlusExpand, "Expandable"), @"  <expand>⊞</expand> Expandable");
    }

    #[test]
    fn test_nested_indent_level_1() {
        let mut state = ListState::default();
        // First item at level 0
        let _ = render_with_state(0, ListBullet::Dash, "Parent", &mut state);
        // Nested item at indent 1
        let nested = render_with_state(1, ListBullet::Dash, "Child", &mut state);
        insta::assert_snapshot!(nested, @"    <dash>◦</dash> Child");
    }

    #[test]
    fn test_nested_indent_level_2() {
        let mut state = ListState::default();
        let _ = render_with_state(0, ListBullet::Dash, "Level 0", &mut state);
        let _ = render_with_state(1, ListBullet::Dash, "Level 1", &mut state);
        let level2 = render_with_state(2, ListBullet::Dash, "Level 2", &mut state);
        insta::assert_snapshot!(level2, @"      <dash>▪</dash> Level 2");
    }

    #[test]
    fn test_bullet_cycling() {
        let mut state = ListState::default();
        let l0 = render_with_state(0, ListBullet::Dash, "L0", &mut state);
        let l1 = render_with_state(1, ListBullet::Dash, "L1", &mut state);
        let l2 = render_with_state(2, ListBullet::Dash, "L2", &mut state);
        let l3 = render_with_state(3, ListBullet::Dash, "L3", &mut state);
        let l4 = render_with_state(4, ListBullet::Dash, "L4", &mut state); // cycles back

        assert!(l0.contains("•"), "Level 0 should use •");
        assert!(l1.contains("◦"), "Level 1 should use ◦");
        assert!(l2.contains("▪"), "Level 2 should use ▪");
        assert!(l3.contains("‣"), "Level 3 should use ‣");
        assert!(l4.contains("•"), "Level 4 should cycle back to •");
    }

    #[test]
    fn test_inline_bold() {
        insta::assert_snapshot!(render(0, ListBullet::Dash, "Item with **bold** text"), @"  <dash>•</dash> Item with <b>bold</b> text");
    }

    #[test]
    fn test_inline_italic() {
        insta::assert_snapshot!(render(0, ListBullet::Dash, "Item with *italic* text"), @"  <dash>•</dash> Item with <i>italic</i> text");
    }

    #[test]
    fn test_inline_code() {
        insta::assert_snapshot!(render(0, ListBullet::Dash, "Item with `code` text"), @"  <dash>•</dash> Item with <code>code</code> text");
    }

    #[test]
    fn test_inline_link() {
        insta::assert_snapshot!(render(0, ListBullet::Dash, "See [link](https://example.com)"), @r#"  <dash>•</dash> See <a href="https://example.com">link</a>"#);
    }

    #[test]
    fn test_empty_content() {
        insta::assert_snapshot!(render(0, ListBullet::Dash, ""), @"  <dash>•</dash> ");
    }

    #[test]
    fn test_wrapping_long_content() {
        let result = render_with_width(0, ListBullet::Dash, "This is a very long list item that should wrap to multiple lines", 40);
        insta::assert_snapshot!(result, @r"
        <dash>•</dash> This is a very long list item that
          should wrap to multiple lines
        ");
    }

    #[test]
    fn test_list_state_reset() {
        let mut state = ListState::default();
        let _ = render_with_state(0, ListBullet::Ordered(1), "First", &mut state);
        let _ = render_with_state(0, ListBullet::Ordered(1), "Second", &mut state);
        state.reset();
        let after_reset = render_with_state(0, ListBullet::Ordered(1), "New first", &mut state);
        insta::assert_snapshot!(after_reset, @"  <num>1.</num> New first");
    }

    #[test]
    fn test_dedent_resets_nested_numbers() {
        let mut state = ListState::default();
        let _ = render_with_state(0, ListBullet::Ordered(1), "Parent 1", &mut state);
        let _ = render_with_state(1, ListBullet::Ordered(1), "Child 1", &mut state);
        let _ = render_with_state(1, ListBullet::Ordered(1), "Child 2", &mut state);
        let parent2 = render_with_state(0, ListBullet::Ordered(1), "Parent 2", &mut state);
        insta::assert_snapshot!(parent2, @"  <num>2.</num> Parent 2");
    }

    #[test]
    fn test_mixed_ordered_unordered() {
        let mut state = ListState::default();
        let ordered = render_with_state(0, ListBullet::Ordered(1), "Ordered item", &mut state);
        state.reset();
        let mut state2 = ListState::default();
        let unordered = render_with_state(0, ListBullet::Dash, "Unordered item", &mut state2);

        assert!(ordered.contains("<num>"), "Ordered should use number style");
        assert!(unordered.contains("<dash>"), "Unordered should use dash style");
    }

    #[test]
    fn test_list_state_level() {
        let mut state = ListState::default();
        assert_eq!(state.level(), 0);

        state.push(0, false);
        assert_eq!(state.level(), 1);

        state.push(1, false);
        assert_eq!(state.level(), 2);

        state.pop();
        assert_eq!(state.level(), 1);
    }
}
