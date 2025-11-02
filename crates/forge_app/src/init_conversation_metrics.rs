use chrono::{DateTime, Local, Utc};
use forge_domain::Conversation;

/// Initializes conversation metrics with start time
#[derive(Debug, Clone, Copy)]
pub struct InitConversationMetrics {
    current_time: DateTime<Local>,
}

impl InitConversationMetrics {
    pub const fn new(current_time: DateTime<Local>) -> Self {
        Self { current_time }
    }

    pub fn apply(self, mut conversation: Conversation) -> Conversation {
        conversation.metrics.started_at = Some(self.current_time.with_timezone(&Utc));
        conversation
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::ConversationId;

    use super::*;

    #[test]
    fn test_sets_started_at() {
        let current_time = Local::now();
        let conversation = Conversation::new(ConversationId::generate());

        let actual = InitConversationMetrics::new(current_time).apply(conversation);

        assert!(actual.metrics.started_at.is_some());
        let expected_time = current_time.with_timezone(&Utc);
        let actual_time = actual.metrics.started_at.unwrap();

        // Compare timestamps with some tolerance (1 second)
        let diff = (actual_time - expected_time).num_seconds().abs();
        assert!(diff < 1, "Timestamps should be within 1 second");
    }
}
