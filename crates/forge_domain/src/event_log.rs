use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ArcSender, ChatResponse};

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

/// Sender wrapper that intercepts messages and logs them to conversation event
/// log
///
/// This wrapper automatically captures all successful ChatResponse messages
/// and appends them to the event log with timestamps. It transparently
/// forwards all messages to the underlying sender.
#[derive(Clone, Debug)]
pub struct EventLoggingSender {
    inner: ArcSender,
    event_log: Arc<Mutex<Option<ConversationEventLog>>>,
}

impl EventLoggingSender {
    /// Creates a new EventLoggingSender that wraps an existing sender
    ///
    /// # Arguments
    /// * `sender` - The underlying sender to forward messages to
    /// * `event_log` - Shared reference to the conversation's event log
    pub fn new(sender: ArcSender, event_log: Arc<Mutex<Option<ConversationEventLog>>>) -> Self {
        Self { inner: sender, event_log }
    }

    /// Sends a message to the UI and captures it in the event log
    ///
    /// Successful ChatResponse messages are automatically logged with
    /// timestamps. Error results are forwarded but not logged.
    ///
    /// # Errors
    /// Returns an error if the underlying sender fails or if the event log lock
    /// is poisoned
    pub async fn send(&self, message: Result<ChatResponse, anyhow::Error>) -> anyhow::Result<()> {
        // Capture successful messages in event log
        if let Ok(ref chat_response) = message {
            let mut log = self
                .event_log
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to acquire event log lock"))?;

            if let Some(ref mut event_log) = *log {
                event_log.push(TimestampedEvent::new(chat_response.clone()));
            } else {
                // Initialize event log if it doesn't exist
                let mut new_log = ConversationEventLog::new();
                new_log.push(TimestampedEvent::new(chat_response.clone()));
                *log = Some(new_log);
            }
        }

        // Forward to UI
        self.inner
            .send(message)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))
    }

    /// Returns a reference to the underlying sender
    pub fn inner(&self) -> &ArcSender {
        &self.inner
    }

    /// Try to send a message without waiting (non-async version)
    ///
    /// This is a non-blocking version of send. Successful messages are logged.
    ///
    /// # Errors
    /// Returns an error if the channel is full or closed, or if the event log
    /// lock is poisoned
    pub fn try_send(
        &self,
        message: Result<ChatResponse, anyhow::Error>,
    ) -> Result<(), tokio::sync::mpsc::error::TrySendError<Result<ChatResponse, anyhow::Error>>>
    {
        // Capture successful messages in event log
        if let Ok(ref chat_response) = message
            && let Ok(mut log) = self.event_log.lock()
        {
            if let Some(ref mut event_log) = *log {
                event_log.push(TimestampedEvent::new(chat_response.clone()));
            } else {
                let mut new_log = ConversationEventLog::new();
                new_log.push(TimestampedEvent::new(chat_response.clone()));
                *log = Some(new_log);
            }
        }

        // Forward to UI
        self.inner.try_send(message)
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

    #[tokio::test]
    async fn test_event_logging_sender_captures_successful_messages() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        let event_log = Arc::new(Mutex::new(Some(ConversationEventLog::new())));
        let sender = EventLoggingSender::new(tx, event_log.clone());

        let message = ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("test".to_string()),
        };

        sender.send(Ok(message.clone())).await.unwrap();

        // Verify message forwarded to inner sender
        let received = rx.recv().await.unwrap().unwrap();
        assert!(matches!(received, ChatResponse::TaskMessage { .. }));

        // Verify message captured in event log
        let log = event_log.lock().unwrap();
        assert_eq!(log.as_ref().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_event_logging_sender_ignores_error_messages() {
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let event_log = Arc::new(Mutex::new(Some(ConversationEventLog::new())));
        let sender = EventLoggingSender::new(tx, event_log.clone());

        let error = anyhow::anyhow!("test error");
        let _result = sender.send(Err(error)).await;

        // Error should be forwarded (but receiver is dropped, so send fails)
        // Event log should remain empty
        let log = event_log.lock().unwrap();
        assert_eq!(log.as_ref().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_event_logging_sender_initializes_event_log() {
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let event_log = Arc::new(Mutex::new(None)); // Start with None
        let sender = EventLoggingSender::new(tx, event_log.clone());

        let message = ChatResponse::TaskMessage {
            content: ChatResponseContent::PlainText("test".to_string()),
        };

        sender.send(Ok(message)).await.unwrap();

        // Event log should be initialized and contain message
        let log = event_log.lock().unwrap();
        assert!(log.is_some());
        assert_eq!(log.as_ref().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_event_logging_sender_thread_safe() {
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        let event_log = Arc::new(Mutex::new(Some(ConversationEventLog::new())));
        let sender = EventLoggingSender::new(tx, event_log.clone());

        // Send multiple messages concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let sender = sender.clone();
                tokio::spawn(async move {
                    let message = ChatResponse::TaskMessage {
                        content: ChatResponseContent::PlainText(format!("msg{}", i)),
                    };
                    sender.send(Ok(message)).await
                })
            })
            .collect();

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // All messages should be captured
        let log = event_log.lock().unwrap();
        assert_eq!(log.as_ref().unwrap().len(), 10);
    }
}
