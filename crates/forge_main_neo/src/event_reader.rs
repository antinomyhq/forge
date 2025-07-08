use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use tokio::sync::mpsc::Sender;

use crate::domain::Action;

pub struct EventReader {
    timeout: Duration,
}

impl EventReader {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Filters out unwanted events to prevent duplication on Windows
    fn should_process_event(event: &Event) -> bool {
        match event {
            Event::Key(key_event) => {
                // Only process KeyDown events to avoid duplication on Windows
                // KeyUp and KeyRepeat events are filtered out
                matches!(key_event.kind, KeyEventKind::Press)
            }
            // Process all other event types (mouse, resize, etc.)
            _ => true,
        }
    }

    pub async fn init(&self, tx: Sender<anyhow::Result<Action>>) {
        let timeout = self.timeout;
        tokio::spawn(async move {
            while !tx.is_closed() {
                if event::poll(timeout).unwrap() && !tx.is_closed() {
                    let e = event::read().unwrap();

                    // Filter out unwanted events to prevent duplication
                    if Self::should_process_event(&e) {
                        tx.send(Ok(e.into())).await.unwrap();
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    #[test]
    fn test_should_process_key_press_events() {
        let fixture = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        ));

        let actual = EventReader::should_process_event(&fixture);
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_not_process_key_release_events() {
        let fixture = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));

        let actual = EventReader::should_process_event(&fixture);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_not_process_key_repeat_events() {
        let fixture = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));

        let actual = EventReader::should_process_event(&fixture);
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_should_process_non_key_events() {
        let fixture = Event::Resize(80, 24);

        let actual = EventReader::should_process_event(&fixture);
        let expected = true;

        assert_eq!(actual, expected);
    }
}
