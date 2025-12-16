use forge_domain::{Agent, Context, Transformer};

use crate::compact::CompactRange;

/// Transformer that compacts context when necessary before sending to LLM
pub struct CompactionTransformer<C> {
    agent: Agent,
    compactor: Option<C>,
}

impl<C: CompactRange> CompactionTransformer<C> {
    /// Creates a new CompactionTransformer
    ///
    /// # Arguments
    ///
    /// * `agent` - The agent configuration containing compaction settings
    /// * `compactor` - The compaction service implementation
    pub fn new(agent: Agent, compactor: Option<C>) -> Self {
        Self { agent, compactor }
    }

    fn find_last_breakpoint(&self, ctx: &Context) -> Option<usize> {
        if ctx.messages.is_empty() {
            return None;
        }

        let mut last_bp: Option<usize> = None;
        let mut acc_ctx = Context::default();

        for (i, msg) in ctx.messages.iter().enumerate() {
            acc_ctx = acc_ctx.add_message(msg.message.clone());

            let token_count = *acc_ctx.token_count();
            if self.agent.should_compact(&acc_ctx, token_count) {
                last_bp = Some(i);
                acc_ctx = Context::default();
            }
        }

        last_bp
    }
}

impl<C: CompactRange> Transformer for CompactionTransformer<C> {
    type Value = Context;

    fn transform(&mut self, context: Self::Value) -> Self::Value {
        let Some(compactor) = self.compactor.as_ref() else {
            return context;
        };

        let Some(breakpoint) = self.find_last_breakpoint(&context) else {
            tracing::debug!(agent_id = %self.agent.id, "No compaction needed");
            return context;
        };

        match compactor.compact_range(&context, breakpoint) {
            Ok(msg) => {
                let mut compacted_context = Context::default().add_message(msg);

                // Add the remaining messages after breakpoint
                for entry in context.messages.iter().skip(breakpoint + 1) {
                    compacted_context = compacted_context.add_message(entry.message.clone());
                }

                tracing::info!(
                    agent_id = %self.agent.id,
                    original_messages = context.messages.len(),
                    compacted_messages = compacted_context.messages.len(),
                    "Context compacted"
                );

                compacted_context
            }
            Err(e) => {
                tracing::error!(
                    agent_id = %self.agent.id,
                    error = ?e,
                    "Compaction failed, using original context"
                );
                context
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Compact, ContextMessage, ModelId, ProviderId};
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockCompactor;

    impl CompactRange for MockCompactor {
        fn compact_range(&self, _context: &Context, _max: usize) -> anyhow::Result<ContextMessage> {
            // Simple mock: just return a context with fewer messages
            Ok(ContextMessage::user("Compacted summary", None))
        }
    }

    fn test_agent() -> Agent {
        Agent::new(
            "test-agent",
            ProviderId::from("openai".to_string()),
            ModelId::from("gpt-4".to_string()),
        )
        .compact(
            Compact::new()
                .token_threshold(1000usize) // Very low threshold to trigger easily
                .eviction_window(0.5)
                .retention_window(2usize),
        )
    }

    #[test]
    fn test_no_compaction_for_small_context() {
        let agent = test_agent();
        let compactor = MockCompactor;

        let fixture = Context::default()
            .add_message(ContextMessage::user("Message 1", None))
            .add_message(ContextMessage::assistant("Response 1", None, None));

        let mut transformer = CompactionTransformer::new(agent, Some(compactor));
        let actual = transformer.transform(fixture.clone());

        assert_eq!(actual.messages.len(), fixture.messages.len());
    }

    #[test]
    fn test_compaction_with_single_breakpoint() {
        let agent = test_agent();
        let compactor = MockCompactor;

        // Create context with enough messages to trigger compaction
        // The agent is configured with eviction_window=0.5 and retention_window=2
        // This means compaction triggers very easily
        let mut fixture = Context::default();
        for i in 0..50 {
            // Add substantial content to increase token count
            fixture = fixture
                .add_message(ContextMessage::user(
                    format!("User message {} with substantial content to increase token count. This message contains enough text to make sure we hit the compaction threshold quickly. The threshold is set to very low values in the test agent configuration.", i),
                    None,
                ))
                .add_message(ContextMessage::assistant(
                    format!("Assistant response {} with substantial content to increase token count. This response also contains enough text to ensure we accumulate sufficient tokens to trigger the compaction logic.", i),
                    None,
                    None,
                ));
        }

        let mut transformer = CompactionTransformer::new(agent, Some(compactor));
        let actual = transformer.transform(fixture);

        assert_eq!(actual.messages.len(), 1);
    }
}
