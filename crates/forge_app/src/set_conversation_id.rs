use forge_domain::Conversation;

/// Sets the conversation_id on the conversation context
#[derive(Debug, Clone, Copy, Default)]
pub struct SetConversationId;

impl SetConversationId {
    pub fn apply(self, mut conversation: Conversation) -> Conversation {
        let mut p_ctx = conversation.context.take().unwrap_or_default();
        p_ctx.context = p_ctx.context.conversation_id(conversation.id);
        conversation.context(p_ctx)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ConversationId, ParentContext};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_sets_conversation_id() {
        let conversation_id = ConversationId::generate();
        let conversation = Conversation::new(conversation_id)
            .context(ParentContext::default().context(Context::default()));

        let actual = SetConversationId.apply(conversation);

        assert_eq!(
            actual.context.unwrap().conversation_id,
            Some(conversation_id)
        );
    }
}
