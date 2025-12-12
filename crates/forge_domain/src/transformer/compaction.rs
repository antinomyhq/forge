use crate::{Agent, Context, Transformer};

/// Transformer that compacts context when necessary before sending to LLM
///
/// This transformer checks if compaction is needed based on the agent's
/// configuration and applies compaction if required. Unlike other transformers,
/// this one modifies the context by replacing messages with a summary.
///
/// The compaction process:
/// 1. Checks if token count exceeds configured thresholds
/// 2. Identifies sequences of messages that can be compacted
/// 3. Generates a summary of those messages
/// 4. Replaces the sequence with the summary message
pub struct CompactionTransformer<C> {
    agent: Agent,
    compactor: Option<C>,
}

impl<C> CompactionTransformer<C> {
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

/// Trait for context compaction functionality
pub trait ContextCompactor {
    /// Compact the given context
    ///
    /// # Errors
    ///
    /// Returns an error if compaction fails
    fn compact(&self, context: Context, max: bool) -> anyhow::Result<Context>;
}

impl<C: ContextCompactor> Transformer for CompactionTransformer<C> {
    type Value = Context;

    fn transform(&mut self, context: Self::Value) -> Self::Value {
        // Check if compaction is needed
        let token_count = context.token_count();
        if self.agent.should_compact(&context, *token_count)
            && let Some(compactor) = self.compactor.as_ref()
        {
            tracing::info!(agent_id = %self.agent.id, "Compaction triggered by transformer");

            match compactor.compact(context.clone(), false) {
                Ok(compacted) => {
                    tracing::info!(
                        agent_id = %self.agent.id,
                        original_messages = context.messages.len(),
                        compacted_messages = compacted.messages.len(),
                        "Context compacted successfully"
                    );
                    compacted
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
        } else {
            tracing::debug!(agent_id = %self.agent.id, "Compaction not needed");
            context
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{Compact, ContextMessage, ModelId, ProviderId};

    struct MockCompactor;

    impl ContextCompactor for MockCompactor {
        fn compact(&self, _context: Context, _max: bool) -> anyhow::Result<Context> {
            // Simple mock: just return a context with fewer messages
            Ok(Context::default().add_message(ContextMessage::user("Compacted summary", None)))
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
                .eviction_window(5.0)
                .retention_window(10usize),
        )
    }

    #[test]
    fn test_compaction_not_triggered_for_small_context() {
        let agent = test_agent();
        let compactor = MockCompactor;

        // Create a small context that shouldn't trigger compaction
        let fixture = Context::default()
            .add_message(ContextMessage::user("Message 1", None))
            .add_message(ContextMessage::assistant("Response 1", None, None));

        let mut transformer = CompactionTransformer::new(agent, Some(compactor));
        let actual = transformer.transform(fixture.clone());

        // Context should remain unchanged because compaction threshold not reached
        assert_eq!(actual.messages.len(), fixture.messages.len());
    }
}
