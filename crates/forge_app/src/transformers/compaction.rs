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
}

impl<C: CompactRange> Transformer for CompactionTransformer<C> {
    type Value = Context;

    fn transform(&mut self, context: Self::Value) -> Self::Value {
        let Some(compactor) = self.compactor.as_ref() else {
            return context;
        };

        let Some(compact_config) = &self.agent.compact else {
            return context;
        };

        match compactor.compact_range(&context, compact_config) {
            Ok(Some(compacted_context)) => compacted_context,
            Ok(None) => {
                tracing::debug!(agent_id = %self.agent.id, "No compaction needed");
                context
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
    use forge_domain::{Compact, MessagePattern, ModelId, ProviderId};
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockCompactor;

    impl CompactRange for MockCompactor {
        fn compact_range(
            &self,
            context: &Context,
            _compact_config: &Compact,
        ) -> anyhow::Result<Option<Context>> {
            // Simple mock: compact if there are more than 10 messages
            if context.messages.len() <= 10 {
                return Ok(None);
            }

            // Return a simple compacted context with 1 summary message
            Ok(Some(
                Context::default()
                    .add_message(forge_domain::ContextMessage::user("Compacted summary", None)),
            ))
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

    /// Helper to create context from SAURT pattern
    /// s = system, a = assistant, u = user, r = tool result, t = tool call
    fn ctx(pattern: &str) -> Context {
        MessagePattern::new(pattern).build()
    }

    #[test]
    fn test_no_compaction_for_small_context() {
        let agent = test_agent();
        let compactor = MockCompactor;

        let fixture = ctx("ua"); // user, assistant

        let mut transformer = CompactionTransformer::new(agent, Some(compactor));
        let actual = transformer.transform(fixture.clone());

        assert_eq!(actual.messages.len(), fixture.messages.len());
    }

    #[test]
    fn test_compaction_with_threshold_exceeded() {
        let agent = test_agent();
        let compactor = MockCompactor;

        // Create a pattern with many messages to exceed threshold
        // Using the SAURT notation: 50 user-assistant pairs
        let pattern = "ua".repeat(50);
        let fixture = ctx(&pattern);

        let mut transformer = CompactionTransformer::new(agent, Some(compactor));
        let actual = transformer.transform(fixture);

        // MockCompactor returns a single summary message when compaction occurs
        assert_eq!(actual.messages.len(), 1);
    }
}

