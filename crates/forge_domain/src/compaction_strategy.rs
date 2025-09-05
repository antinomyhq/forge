use crate::{Context, Role};

/// Strategy for context compaction that unifies different compaction approaches
#[derive(Debug, Clone)]
pub enum CompactionStrategy {
    /// Retention based on percentage of tokens
    Evict(f64),
    /// Retention based on fixed tokens
    Retain(usize),

    /// Selects the strategy with minimum retention
    Min(Box<CompactionStrategy>, Box<CompactionStrategy>),

    /// Selects the strategy with maximum retention
    Max(Box<CompactionStrategy>, Box<CompactionStrategy>),
}

impl CompactionStrategy {
    /// Create a percentage-based compaction strategy
    pub fn evict(percentage: f64) -> Self {
        Self::Evict(percentage)
    }

    /// Create a preserve-last-N compaction strategy
    pub fn retain(preserve_last_n: usize) -> Self {
        Self::Retain(preserve_last_n)
    }

    pub fn min(self, other: CompactionStrategy) -> Self {
        CompactionStrategy::Min(Box::new(self), Box::new(other))
    }

    pub fn max(self, other: CompactionStrategy) -> Self {
        CompactionStrategy::Max(Box::new(self), Box::new(other))
    }

    /// Convert percentage-based strategy to preserve_last_n equivalent
    /// This simulates the original percentage algorithm to determine how many
    /// messages would be preserved, then returns that as a preserve_last_n
    /// value
    fn to_fixed(&self, context: &Context) -> usize {
        match self {
            CompactionStrategy::Evict(percentage) => {
                let percentage = percentage.min(1.0);
                let total_tokens = context.token_count();
                let mut eviction_budget: usize =
                    (percentage * (*total_tokens) as f64).ceil() as usize;

                let range = context
                    .messages
                    .iter()
                    .enumerate()
                    // Skip system message
                    .filter(|m| !m.1.has_role(Role::System))
                    .find(|(_, m)| {
                        eviction_budget = eviction_budget.saturating_sub(m.token_count_approx());
                        eviction_budget == 0
                    });

                match range {
                    Some((i, _)) => i,
                    None => context.messages.len().saturating_sub(1),
                }
            }
            CompactionStrategy::Retain(fixed) => *fixed,
            CompactionStrategy::Min(a, b) => a.to_fixed(context).min(b.to_fixed(context)),
            CompactionStrategy::Max(a, b) => a.to_fixed(context).max(b.to_fixed(context)),
        }
    }

    /// Find the sequence to compact using the unified algorithm
    pub fn eviction_range(&self, context: &Context) -> Option<(usize, usize)> {
        let retention = self.to_fixed(context);
        find_sequence_preserving_last_n(context, retention)
    }
}

/// Finds a sequence in the context for compaction, starting from the first
/// assistant message and including all messages up to the last possible message
/// (respecting preservation window)
fn find_sequence_preserving_last_n(
    context: &Context,
    max_retention: usize,
) -> Option<(usize, usize)> {
    let messages = &context.messages;
    if messages.is_empty() {
        return None;
    }

    // len will be always > 0
    let length = messages.len();

    // Find the first assistant message index
    let start = messages
        .iter()
        .enumerate()
        .find(|(_, message)| !message.has_role(Role::System))
        .map(|(index, _)| index)?;

    // Don't compact if there's no assistant message
    if start >= length {
        return None;
    }

    // Calculate the end index based on preservation window
    // If we need to preserve all or more messages than we have, there's nothing to
    // compact
    if max_retention >= length {
        return None;
    }

    // Use saturating subtraction to prevent potential overflow
    let mut end = length.saturating_sub(max_retention).saturating_sub(1);

    // Ensure we have at least two messages to create a meaningful summary
    // If start > end or end is invalid, don't compact
    if start > end || end >= length || end.saturating_sub(start) < 1 {
        return None;
    }

    // Don't break between a tool call and its result
    if messages.get(end).is_some_and(|msg| msg.has_tool_call()) {
        // If the last message has a tool call, adjust end to include the tool result
        // This means either not compacting at all, or reducing the end by 1
        if end == start {
            // If start == end and it has a tool call, don't compact
            return None;
        } else {
            // Otherwise reduce end by 1
            return Some((start, end.saturating_sub(1)));
        }
    }

    if messages.get(end).is_some_and(|msg| msg.has_tool_result())
        && messages
            .get(end.saturating_add(1))
            .is_some_and(|msg| msg.has_tool_result())
    {
        // If the last message is a tool result and the next one is also a tool result,
        // we need to adjust the end.
        while end >= start && messages.get(end).is_some_and(|msg| msg.has_tool_result()) {
            end = end.saturating_sub(1);
        }
        end = end.saturating_sub(1);
    }

    // Return the sequence only if it has at least one message
    if end >= start {
        Some((start, end))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::{ContextMessage, ModelId, ToolCallFull, ToolCallId, ToolName, ToolResult};

    fn context_from_pattern(pattern: impl ToString) -> Context {
        let model_id = ModelId::new("gpt-4");
        let pattern = pattern.to_string();

        let tool_call = ToolCallFull {
            name: ToolName::new("read"),
            call_id: Some(ToolCallId::new("call_123")),
            arguments: serde_json::json!({"path": "/test/path"}).into(),
        };

        let tool_result = ToolResult::new(ToolName::new("read"))
            .call_id(ToolCallId::new("call_123"))
            .success(json!({"content": "File content"}).to_string());

        let mut context = Context::default();

        for c in pattern.chars() {
            match c {
                's' => context = context.add_message(ContextMessage::system("System message")),
                'u' => {
                    context = context.add_message(ContextMessage::user(
                        "User message",
                        model_id.clone().into(),
                    ))
                }
                'a' => {
                    context = context.add_message(ContextMessage::assistant(
                        "Assistant message",
                        None,
                        None,
                    ))
                }
                't' => {
                    context = context.add_message(ContextMessage::assistant(
                        "Assistant message with tool call",
                        None,
                        Some(vec![tool_call.clone()]),
                    ))
                }
                'r' => {
                    context = context.add_message(ContextMessage::tool_result(tool_result.clone()))
                }
                _ => panic!("Invalid character in test pattern: {c}"),
            }
        }

        context
    }

    fn seq(pattern: impl ToString, preserve_last_n: usize) -> String {
        let pattern = pattern.to_string();
        let context = context_from_pattern(&pattern);

        let sequence = find_sequence_preserving_last_n(&context, preserve_last_n);

        let mut result = pattern.clone();
        if let Some((start, end)) = sequence {
            result.insert(start, '[');
            result.insert(end + 2, ']');
        }

        result
    }

    #[test]
    fn test_sequence_finding() {
        // Basic compaction scenarios
        let actual = seq("suaaau", 0);
        let expected = "s[uaaau]";
        assert_eq!(actual, expected);

        let actual = seq("sua", 0);
        let expected = "s[ua]";
        assert_eq!(actual, expected);

        let actual = seq("suauaa", 0);
        let expected = "s[uauaa]";
        assert_eq!(actual, expected);

        // Tool call scenarios
        let actual = seq("suttu", 0);
        let expected = "s[uttu]";
        assert_eq!(actual, expected);

        let actual = seq("sutraau", 0);
        let expected = "s[utraau]";
        assert_eq!(actual, expected);

        let actual = seq("utrutru", 0);
        let expected = "[utrutru]";
        assert_eq!(actual, expected);

        let actual = seq("uttarru", 0);
        let expected = "[uttarru]";
        assert_eq!(actual, expected);

        let actual = seq("urru", 0);
        let expected = "[urru]";
        assert_eq!(actual, expected);

        let actual = seq("uturu", 0);
        let expected = "[uturu]";
        assert_eq!(actual, expected);

        // Preservation window scenarios
        let actual = seq("suaaaauaa", 0);
        let expected = "s[uaaaauaa]";
        assert_eq!(actual, expected);

        let actual = seq("suaaaauaa", 3);
        let expected = "s[uaaaa]uaa";
        assert_eq!(actual, expected);

        let actual = seq("suaaaauaa", 5);
        let expected = "s[uaa]aauaa";
        assert_eq!(actual, expected);

        let actual = seq("suaaaauaa", 8);
        let expected = "suaaaauaa";
        assert_eq!(actual, expected);

        let actual = seq("suauaaa", 0);
        let expected = "s[uauaaa]";
        assert_eq!(actual, expected);

        let actual = seq("suauaaa", 2);
        let expected = "s[uaua]aa";
        assert_eq!(actual, expected);

        let actual = seq("suauaaa", 1);
        let expected = "s[uauaa]a";
        assert_eq!(actual, expected);

        // Tool call atomicity preservation
        let actual = seq("sutrtrtra", 0);
        let expected = "s[utrtrtra]";
        assert_eq!(actual, expected);

        let actual = seq("sutrtrtra", 1);
        let expected = "s[utrtrtr]a";
        assert_eq!(actual, expected);

        let actual = seq("sutrtrtra", 2);
        let expected = "s[utrtr]tra";
        assert_eq!(actual, expected);

        // Parallel tool calls
        let actual = seq("sutrtrtrra", 2);
        let expected = "s[utrtr]trra";
        assert_eq!(actual, expected);

        let actual = seq("sutrtrtrra", 3);
        let expected = "s[utrtr]trra";
        assert_eq!(actual, expected);

        let actual = seq("sutrrtrrtrra", 5);
        let expected = "s[utrr]trrtrra";
        assert_eq!(actual, expected);

        let actual = seq("sutrrrrrra", 2);
        let expected = "s[u]trrrrrra";
        assert_eq!(actual, expected);

        // Conversation patterns
        let actual = seq("suauauaua", 0);
        let expected = "s[uauauaua]";
        assert_eq!(actual, expected);

        let actual = seq("suauauaua", 2);
        let expected = "s[uauaua]ua";
        assert_eq!(actual, expected);

        let actual = seq("suauauaua", 6);
        let expected = "s[ua]uauaua";
        assert_eq!(actual, expected);

        let actual = seq("sutruaua", 0);
        let expected = "s[utruaua]";
        assert_eq!(actual, expected);

        let actual = seq("sutruaua", 3);
        let expected = "s[utru]aua";
        assert_eq!(actual, expected);

        // Special cases
        let actual = seq("saua", 0);
        let expected = "s[aua]";
        assert_eq!(actual, expected);

        let actual = seq("suaut", 0);
        let expected = "s[uau]t";
        assert_eq!(actual, expected);

        // Edge cases
        let actual = seq("", 0);
        let expected = "";
        assert_eq!(actual, expected);

        let actual = seq("s", 0);
        let expected = "s";
        assert_eq!(actual, expected);

        let actual = seq("sua", 3);
        let expected = "sua";
        assert_eq!(actual, expected);

        let actual = seq("ut", 0);
        let expected = "[u]t";
        assert_eq!(actual, expected);

        let actual = seq("suuu", 0);
        let expected = "s[uuu]";
        assert_eq!(actual, expected);

        let actual = seq("ut", 1);
        let expected = "ut";
        assert_eq!(actual, expected);

        let actual = seq("ua", 0);
        let expected = "[ua]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_compact_strategy_to_fixed_conversion() {
        // Create a simple context using 'sua' DSL: system, user, assistant
        let fixture = context_from_pattern("sua");

        // Test Percentage strategy conversion
        // Context: System (0 tokens), User (3 tokens), Assistant (5 tokens) = 8 total
        // tokens Eviction budget: 40% of 8 = 4 tokens (rounded up)
        // Calculation:
        // - Skip system message (0 tokens)
        // - User message: 3 tokens → budget: 4 - 3 = 1 token remaining
        // - Assistant message: 5 tokens → budget: 1 - 5 = 0 (saturating_sub), budget
        //   exhausted
        // Result: Can evict 1 message (User), so preserve last 2 messages (System +
        // Assistant)
        let percentage_strategy = CompactionStrategy::evict(0.4);
        let actual = percentage_strategy.to_fixed(&fixture);
        let expected = 2; // Preserve last 2 messages
        assert_eq!(actual, expected);

        // Test PreserveLastN strategy
        let preserve_strategy = CompactionStrategy::retain(3);
        let actual = preserve_strategy.to_fixed(&fixture);
        let expected = 3;
        assert_eq!(actual, expected);

        // Test invalid percentage (gets clamped to 1.0 = 100%)
        // With 100% eviction budget (8 tokens), we can evict all non-system messages
        // This leaves us with just the system message, so preserve last 1 message
        let invalid_strategy = CompactionStrategy::evict(1.5);
        let actual = invalid_strategy.to_fixed(&fixture);
        let expected = 2; // Still preserve 2 messages because fallback logic
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_compact_strategy_conversion_equivalence() {
        // Create context using DSL: user, assistant, user, assistant, user
        let fixture = context_from_pattern("uauau");

        let percentage_strategy = CompactionStrategy::evict(0.6);
        let actual_sequence = percentage_strategy.eviction_range(&fixture);

        // Convert percentage to preserve_last_n and test equivalence
        let preserve_last_n = percentage_strategy.to_fixed(&fixture);
        let preserve_strategy = CompactionStrategy::retain(preserve_last_n);
        let expected_sequence = preserve_strategy.eviction_range(&fixture);
        assert_eq!(actual_sequence, expected_sequence);
    }

    #[test]
    fn test_compact_strategy_api_usage_example() {
        // Create context using DSL: user, assistant, user, assistant
        let fixture = context_from_pattern("uaua");

        // Use percentage-based strategy
        let percentage_strategy = CompactionStrategy::evict(0.4);
        percentage_strategy.to_fixed(&fixture);

        // Use fixed window strategy - preserve last 1 message, so we can compact the
        // first 3
        let preserve_strategy = CompactionStrategy::retain(1);
        let actual_sequence = preserve_strategy.eviction_range(&fixture);
        let expected = Some((0, 2));
        assert_eq!(actual_sequence, expected);
    }

    #[test]
    fn test_empty_context_no_overflow() {
        // Test that empty context doesn't cause overflow
        let empty_context = Context::default();

        let percentage_strategy = CompactionStrategy::evict(0.4);
        let actual = percentage_strategy.to_fixed(&empty_context);
        let expected = 0; // Should be 0 for empty context (saturating_sub(1) on 0 = 0)
        assert_eq!(actual, expected);

        let actual_range = percentage_strategy.eviction_range(&empty_context);
        assert_eq!(actual_range, None); // Should return None for empty context
    }

    #[test]
    fn test_single_message_context_no_overflow() {
        // Test that single message context doesn't cause overflow
        let single_context = context_from_pattern("s");

        let percentage_strategy = CompactionStrategy::evict(0.4);
        let actual = percentage_strategy.to_fixed(&single_context);
        let expected = 0; // Should be 0 (1 - 1 = 0 with saturating_sub)
        assert_eq!(actual, expected);

        let actual_range = percentage_strategy.eviction_range(&single_context);
        assert_eq!(actual_range, None); // Should return None for single system message
    }
}
