use forge_domain::{Agent, Context, Transformer};

use crate::compact::Compactor;

/// Transformer that compacts context when necessary before sending to LLM
pub struct CompactionTransformer {
    agent: Agent,
    compactor: Option<Compactor>,
}

impl CompactionTransformer {
    /// Creates a new CompactionTransformer
    ///
    /// # Arguments
    ///
    /// * `agent` - The agent configuration containing compaction settings
    /// * `compactor` - The compaction service implementation
    pub fn new(agent: Agent, compactor: Option<Compactor>) -> Self {
        Self { agent, compactor }
    }
}

impl Transformer for CompactionTransformer {
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
    use fake::{Fake, Faker};
    use forge_domain::{Compact, Environment, MessagePattern, ModelId, ProviderId};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::compact::Compactor;

    fn test_environment() -> Environment {
        let env: Environment = Faker.fake();
        env.cwd(std::path::PathBuf::from("/test/working/dir"))
    }

    fn test_agent() -> Agent {
        Agent::new(
            "test-agent",
            ProviderId::from("openai".to_string()),
            ModelId::from("gpt-4".to_string()),
        )
        .compact(
            Compact::new()
                .message_threshold(10usize) // Trigger compaction after 10 messages
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
        let environment = test_environment();
        let compactor = Compactor::new(agent.compact.clone().unwrap(), environment);

        let fixture = ctx("ua"); // user, assistant

        let mut transformer = CompactionTransformer::new(agent, Some(compactor));
        let actual = transformer.transform(fixture.clone());

        assert_eq!(actual.messages.len(), fixture.messages.len());
    }

    #[test]
    fn test_compaction_with_threshold_exceeded() {
        let agent = test_agent();
        let environment = test_environment();
        let compactor = Compactor::new(agent.compact.clone().unwrap(), environment);

        // Create a pattern with many messages to exceed threshold
        // Using the SAURT notation: 50 user-assistant pairs
        let pattern = "ua".repeat(50);
        let fixture = ctx(&pattern);

        let mut transformer = CompactionTransformer::new(agent, Some(compactor));
        let actual = transformer.transform(fixture.clone());

        // Real compactor should reduce the message count when compaction occurs
        // The exact count depends on the compaction logic, but it should be less
        assert!(
            actual.messages.len() < fixture.messages.len(),
            "Expected compaction to reduce message count from {} to less, but got {}",
            fixture.messages.len(),
            actual.messages.len()
        );
    }
}
