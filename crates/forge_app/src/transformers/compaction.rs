use forge_domain::{Context, Environment, Transformer};

use crate::compact::Compactor;

/// Transformer that compacts context when necessary before sending to LLM
pub struct CompactionTransformer {
    compactor: Compactor,
}

impl CompactionTransformer {
    /// Creates a new CompactionTransformer
    ///
    /// # Arguments
    ///
    /// * `compact` - The compaction configuration
    /// * `env` - The environment for the compactor
    pub fn new(compact: forge_domain::Compact, env: Environment) -> Self {
        Self {
            compactor: Compactor::new(compact, env),
        }
    }
}

impl Transformer for CompactionTransformer {
    type Value = Context;

    fn transform(&mut self, context: Self::Value) -> Self::Value {
        match self.compactor.compact_range(&context) {
            Ok(Some(compacted_context)) => {
                tracing::debug!("Compaction completed");
                compacted_context
            }
            Ok(None) => {
                tracing::debug!("No compaction needed");
                context
            }
            Err(e) => {
                tracing::error!(
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
    use forge_domain::{Compact, Environment, MessagePattern};
    use pretty_assertions::assert_eq;

    use super::*;

    fn test_environment() -> Environment {
        let env: Environment = Faker.fake();
        env.cwd(std::path::PathBuf::from("/test/working/dir"))
    }

    fn test_compact() -> Compact {
        Compact::new()
            .message_threshold(10usize) // Trigger compaction after 10 messages
            .eviction_window(0.5)
            .retention_window(2usize)
    }

    /// Helper to create context from SAURT pattern
    /// s = system, a = assistant, u = user, r = tool result, t = tool call
    fn ctx(pattern: &str) -> Context {
        MessagePattern::new(pattern).build()
    }

    #[test]
    fn test_no_compaction_for_small_context() {
        let compact = test_compact();
        let environment = test_environment();

        let fixture = ctx("ua"); // user, assistant

        let mut transformer = CompactionTransformer::new(compact, environment);
        let actual = transformer.transform(fixture.clone());

        assert_eq!(actual.messages.len(), fixture.messages.len());
    }

    #[test]
    fn test_compaction_with_threshold_exceeded() {
        let compact = test_compact();
        let environment = test_environment();

        // Create a pattern with many messages to exceed threshold
        // Using the SAURT notation: 50 user-assistant pairs
        let pattern = "ua".repeat(50);
        let fixture = ctx(&pattern);

        let mut transformer = CompactionTransformer::new(compact, environment);
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
