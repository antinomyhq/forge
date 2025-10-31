use forge_domain::{Agent, Conversation};

/// Applies tunable parameters from agent to conversation context
#[derive(Debug, Clone)]
pub struct ApplyTunableParameters {
    agent: Agent,
}

impl ApplyTunableParameters {
    pub const fn new(agent: Agent) -> Self {
        Self { agent }
    }

    pub fn apply(self, mut conversation: Conversation) -> Conversation {
        let mut ctx = conversation.context.take().unwrap_or_default();

        if let Some(temperature) = self.agent.temperature {
            ctx = ctx.temperature(temperature);
        }
        if let Some(top_p) = self.agent.top_p {
            ctx = ctx.top_p(top_p);
        }
        if let Some(top_k) = self.agent.top_k {
            ctx = ctx.top_k(top_k);
        }
        if let Some(max_tokens) = self.agent.max_tokens {
            ctx = ctx.max_tokens(max_tokens.value() as usize);
        }
        if let Some(ref reasoning) = self.agent.reasoning {
            ctx = ctx.reasoning(reasoning.clone());
        }

        conversation.context(ctx)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{AgentId, Context, ConversationId, MaxTokens, Temperature};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_apply_sets_parameters() {
        let agent = Agent::new(AgentId::new("test"))
            .temperature(Temperature::new(0.7).unwrap())
            .max_tokens(MaxTokens::new(1000).unwrap());
        let conversation =
            Conversation::new(ConversationId::generate()).context(Context::default());

        let actual = ApplyTunableParameters::new(agent).apply(conversation);

        let ctx = actual.context.unwrap();
        assert_eq!(ctx.temperature, Some(Temperature::new(0.7).unwrap()));
        assert_eq!(ctx.max_tokens, Some(1000));
    }
}
