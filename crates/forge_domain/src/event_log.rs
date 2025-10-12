use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ChatResponse;

/// Event log that captures all ChatResponse events with timestamps
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ConversationEventLog {
    pub events: Vec<TimestampedEvent>,
}

impl ConversationEventLog {
    /// Creates a new empty event log
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new timestamped event to the log
    pub fn push(&mut self, event: TimestampedEvent) {
        self.events.push(event);
    }

    /// Returns true if the event log is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Returns the number of events in the log
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Returns an iterator over the events
    pub fn iter(&self) -> impl Iterator<Item = &TimestampedEvent> {
        self.events.iter()
    }
}

/// A ChatResponse event with timestamp
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TimestampedEvent {
    pub timestamp: DateTime<Utc>,
    pub event: ChatResponse,
}

impl TimestampedEvent {
    /// Creates a new timestamped event with the current time
    pub fn new(event: ChatResponse) -> Self {
        Self { timestamp: Utc::now(), event }
    }

    /// Creates a new timestamped event with a specific timestamp
    pub fn with_timestamp(event: ChatResponse, timestamp: DateTime<Utc>) -> Self {
        Self { timestamp, event }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ChatResponseContent;

    #[test]
    fn test_timestamped_event_creation() {
        let event = ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("test message".to_string()),
        };
        let timestamped = TimestampedEvent::new(event.clone());

        assert!(timestamped.timestamp <= Utc::now());
        match timestamped.event {
            ChatResponse::TaskMessage { content } => {
                assert_eq!(
                    content,
                    ChatResponseContent::PlainText("test message".to_string())
                );
            }
            _ => panic!("Expected TaskMessage"),
        }
    }

    #[test]
    fn test_timestamped_event_with_specific_timestamp() {
        let event = ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("test".to_string()),
        };
        let timestamp = Utc::now();
        let timestamped = TimestampedEvent::with_timestamp(event, timestamp);

        assert_eq!(timestamped.timestamp, timestamp);
    }

    #[test]
    fn test_event_log_default() {
        let log = ConversationEventLog::default();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn test_event_log_push() {
        let mut log = ConversationEventLog::new();
        let event = TimestampedEvent::new(ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("test".to_string()),
        });

        log.push(event);

        assert!(!log.is_empty());
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn test_event_log_iter() {
        let mut log = ConversationEventLog::new();
        log.push(TimestampedEvent::new(ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("msg1".to_string()),
        }));
        log.push(TimestampedEvent::new(ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("msg2".to_string()),
        }));

        let events: Vec<_> = log.iter().collect();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_event_log_serialization() {
        let mut log = ConversationEventLog::new();
        let timestamp = Utc::now();
        log.push(TimestampedEvent::with_timestamp(
            ChatResponse::TaskMessage {
                content: ChatResponseContent::PlainText("test message".to_string()),
            },
            timestamp,
        ));

        let json = serde_json::to_string(&log).unwrap();
        let deserialized: ConversationEventLog = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.len(), 1);
        assert_eq!(deserialized.events[0].timestamp, timestamp);
    }
}
