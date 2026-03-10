use forge_domain::{Agent, Context, Environment, Transformer};
use tracing::info;

use crate::compact::Compactor;

pub struct Compaction {
    agent: Agent,
    environment: Environment,
}

impl Compaction {
    /// Creates a new compaction handler
    ///
    /// # Arguments
    /// * `agent` - The agent configuration containing compaction settings
    /// * `environment` - The environment configuration
    pub fn new(agent: Agent, environment: Environment) -> Self {
        Self { agent, environment }
    }
}

impl Transformer for Compaction {
    type Value = Context;
    fn transform(&mut self, context: Self::Value) -> Self::Value {
        info!(agent_id = %self.agent.id, "Compaction triggered");
        let msg_len = context.messages.len();
        let mut running_context = context.clone().messages(Vec::default());
        let compactor = Compactor::new(self.agent.compact.clone(), self.environment.clone());
        for idx in 0..=msg_len {
            if let Some(entry) = context.messages.get(idx).cloned() {
                running_context.messages.push(entry);
                running_context = compactor.compact(running_context, false).unwrap();
            }
        }
        running_context
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Compact, Context, ContextMessage, MessagePattern, Role};

    use super::*;

    fn compactor(retention: usize) -> Compactor {
        use fake::{Fake, Faker};
        let env: Environment = Faker.fake();
        Compactor::new(
            Compact::new().retention_window(retention),
            env.cwd(std::path::PathBuf::from("/test/working/dir")),
        )
    }

    /// One-line label per message: `[System] Message 1` or `[Compacted]`.
    fn describe(ctx: &Context) -> Vec<String> {
        ctx.messages
            .iter()
            .map(|e| {
                let text = e.content().unwrap_or("");
                if text.contains("summary frames") {
                    return "[Compacted]".to_string();
                }
                let role = match () {
                    _ if e.has_role(Role::System) => "System",
                    _ if e.has_role(Role::Assistant) => "Assistant",
                    _ if e.has_tool_result() => "ToolResult",
                    _ => "User",
                };
                format!("[{role}] {}", &text[..text.len().min(30)])
            })
            .collect()
    }

    /// Compacts different-sized conversations in a single shot.
    #[test]
    fn test_single_compaction() {
        let c = compactor(2);
        let cases: Vec<(&str, Vec<String>)> = [
            ("uaua", "4 messages"),
            ("uauauaua", "8 messages"),
            ("uauauauauaua", "12 messages"),
            ("suauauauauau", "with system"),
        ]
        .into_iter()
        .map(|(pat, label)| {
            let ctx = MessagePattern::new(pat).build();
            let compacted = c.compact(ctx, false).unwrap();
            (label, describe(&compacted))
        })
        .collect();

        insta::assert_yaml_snapshot!(cases);
    }

    /// Multi-round conversation: start with a seed, compact, then add
    /// a user+assistant pair each round and compact again.
    #[test]
    fn test_multi_round_evolution() {
        let c = compactor(2);
        let seed = MessagePattern::new("suauauau").build();

        let mut rounds: Vec<(String, Vec<String>)> = Vec::new();
        rounds.push(("before compaction".into(), describe(&seed)));

        let mut current = c.compact(seed, false).unwrap();
        rounds.push(("request 1".into(), describe(&current)));

        for r in 2..=5 {
            current = current
                .add_message(ContextMessage::user(format!("Question {r}"), None))
                .add_message(ContextMessage::assistant(
                    format!("Answer {r}"),
                    None,
                    None,
                    None,
                ));
            current = c.compact(current, false).unwrap();
            rounds.push((format!("request {r}"), describe(&current)));
        }

        insta::assert_yaml_snapshot!(rounds);
    }
}
