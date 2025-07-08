use edtui::EditorEventHandler;
use forge_api::ChatResponse;
use ratatui::crossterm::event::KeyEventKind;

use crate::domain::update_key_event::handle_key_event;
use crate::domain::{Action, Command, State};

pub fn update(state: &mut State, action: impl Into<Action>) -> Command {
    let action = action.into();
    match action {
        Action::Initialize => Command::ReadWorkspace,
        Action::Workspace { current_dir, current_branch } => {
            // TODO: can simply get workspace object from the action
            state.workspace.current_dir = current_dir;
            state.workspace.current_branch = current_branch;
            Command::Empty
        }
        Action::CrossTerm(event) => match event {
            ratatui::crossterm::event::Event::FocusGained => Command::Empty,
            ratatui::crossterm::event::Event::FocusLost => Command::Empty,
            ratatui::crossterm::event::Event::Key(key_event) => {
                // Filter out unwanted key events to prevent duplication on Windows
                // Only process KeyPress events, ignore KeyRelease and KeyRepeat
                if matches!(key_event.kind, KeyEventKind::Press) {
                    handle_key_event(state, key_event)
                } else {
                    Command::Empty
                }
            }
            ratatui::crossterm::event::Event::Mouse(event) => {
                EditorEventHandler::default().on_mouse_event(event, &mut state.editor);
                Command::Empty
            }
            ratatui::crossterm::event::Event::Paste(_) => Command::Empty,
            ratatui::crossterm::event::Event::Resize(_, _) => Command::Empty,
        },
        Action::ChatResponse(response) => {
            if let ChatResponse::Text { ref text, is_complete, .. } = response
                && is_complete
                && !text.trim().is_empty()
            {
                state.show_spinner = false
            }
            state.add_assistant_message(response);
            if let Some(ref time) = state.timer
                && !state.show_spinner
            {
                let id = time.id.clone();
                state.timer = None;
                return Command::ClearInterval { id };
            }
            Command::Empty
        }
        Action::ConversationInitialized(conversation_id) => {
            state.conversation.init_conversation(conversation_id);
            Command::Empty
        }
        Action::IntervalTick(timer) => {
            state.spinner.calc_next();
            // For now, interval ticks don't trigger any state changes or commands
            // This could be extended to update a timer display or trigger other actions
            state.timer = Some(timer.to_owned());
            Command::Empty
        }
        Action::InterruptStream => {
            // Cancel the ongoing stream if one exists
            if let Some(ref token) = state.stream_cancellation_token {
                token.cancel();
                state.stream_cancellation_token = None;
            }
            // Stop showing spinner and clear any ongoing streaming
            state.show_spinner = false;
            if let Some(ref time) = state.timer {
                let id = time.id.clone();
                state.timer = None;
                return Command::ClearInterval { id };
            }
            Command::Empty
        }
        Action::StartStream(cancellation_token) => {
            // Store the cancellation token for this stream
            state.stream_cancellation_token = Some(cancellation_token);
            Command::Empty
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::*;
    use crate::domain::EditorStateExt;

    #[test]
    fn test_update_processes_key_press_events() {
        let mut fixture_state = State::default();
        // Set editor to Insert mode so text input works
        fixture_state.editor.mode = edtui::EditorMode::Insert;

        let fixture_action = Action::CrossTerm(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        )));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);

        let actual_editor_text = fixture_state.editor.get_text();
        let expected_editor_text = "a".to_string();
        assert_eq!(actual_editor_text, expected_editor_text);
    }

    #[test]
    fn test_update_filters_out_key_release_events() {
        let mut fixture_state = State::default();
        let initial_editor_text = fixture_state.editor.get_text();
        let fixture_action = Action::CrossTerm(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);

        let actual_editor_text = fixture_state.editor.get_text();
        let expected_editor_text = initial_editor_text;
        assert_eq!(actual_editor_text, expected_editor_text);
    }

    #[test]
    fn test_update_filters_out_key_repeat_events() {
        let mut fixture_state = State::default();
        let initial_editor_text = fixture_state.editor.get_text();
        let fixture_action = Action::CrossTerm(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        )));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);

        let actual_editor_text = fixture_state.editor.get_text();
        let expected_editor_text = initial_editor_text;
        assert_eq!(actual_editor_text, expected_editor_text);
    }

    #[test]
    fn test_update_processes_non_key_events() {
        let mut fixture_state = State::default();
        let initial_editor_text = fixture_state.editor.get_text();
        let fixture_action = Action::CrossTerm(Event::Resize(80, 24));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        // Assert on command output
        assert_eq!(actual_command, expected_command);

        let actual_editor_text = fixture_state.editor.get_text();
        let expected_editor_text = initial_editor_text;
        assert_eq!(actual_editor_text, expected_editor_text);
    }

    #[test]
    fn test_interrupt_stream_action_stops_spinner_and_clears_timer() {
        let mut fixture_state = State::default();
        // Set up state as if streaming is active
        fixture_state.show_spinner = true;
        let cancellation_token = tokio_util::sync::CancellationToken::new();
        fixture_state.timer = Some(crate::domain::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_millis(100),
            id: crate::domain::TimerId::from(cancellation_token),
        });

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);

        // Check that a ClearInterval command was returned (we can't compare TimerId
        // directly)
        match actual_command {
            Command::ClearInterval { .. } => {
                // Success - the command type is correct
            }
            _ => panic!("Expected Command::ClearInterval, got {:?}", actual_command),
        }

        assert!(!fixture_state.show_spinner);
        assert!(fixture_state.timer.is_none());
    }

    #[test]
    fn test_interrupt_stream_action_when_no_timer_active() {
        let mut fixture_state = State::default();
        fixture_state.show_spinner = true;
        fixture_state.timer = None;

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(!fixture_state.show_spinner);
        assert!(fixture_state.timer.is_none());
    }

    #[test]
    fn test_start_stream_action_stores_cancellation_token() {
        let mut fixture_state = State::default();
        let cancellation_token = tokio_util::sync::CancellationToken::new();

        let fixture_action = Action::StartStream(cancellation_token.clone());

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.stream_cancellation_token.is_some());
    }

    #[test]
    fn test_interrupt_stream_action_cancels_stream_token() {
        let mut fixture_state = State::default();
        let cancellation_token = tokio_util::sync::CancellationToken::new();
        fixture_state.stream_cancellation_token = Some(cancellation_token.clone());
        fixture_state.show_spinner = true;

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(!fixture_state.show_spinner);
        assert!(fixture_state.stream_cancellation_token.is_none());
        assert!(cancellation_token.is_cancelled());
    }
}
