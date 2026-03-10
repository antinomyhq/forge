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
                if self
                    .agent
                    .compact
                    .should_compact(&running_context, *running_context.token_count())
                {
                    running_context = compactor.compact(running_context, false).unwrap();
                }
            }
        }
        running_context
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{AgentId, Compact, Context, MessagePattern, ModelId, ProviderId, Role};

    use super::*;

    fn compaction(retention: usize, message_thresh: usize) -> impl Transformer<Value = Context> {
        use fake::{Fake, Faker};
        let env: Environment = Faker.fake();
        let env = env.cwd(std::path::PathBuf::from("/test/working/dir"));
        let compact = Compact::new()
            .message_threshold(message_thresh)
            .retention_window(retention);
        let agent = Agent::new(
            AgentId::new("test"),
            ProviderId::ANTHROPIC,
            ModelId::new("test-model"),
        )
        .compact(compact.clone());
        Compaction::new(agent, env)
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

    /// Multi-round conversation: each round builds the full lossless context
    /// from the pattern and passes it through the Compaction transformer.
    /// The transformer is pure -- same input always produces same output.
    #[test]
    fn test_multi_round_evolution() {
        let mut c = compaction(2, 10);
        let mut base = String::from("s");
        let mut rounds: Vec<(String, Vec<String>)> = Vec::new();
        for i in 1..=7 {
            base.push_str("au");
            let seed = MessagePattern::new(base.clone()).build();
            let current = c.transform(seed);
            rounds.push((format!("request {}", i), describe(&current)));
        }

        insta::assert_yaml_snapshot!(rounds);
    }
}
