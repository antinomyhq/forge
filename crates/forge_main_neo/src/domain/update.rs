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
            if let ChatResponse::TaskMessage { ref text, .. } = response
                && !text.trim().is_empty()
            {
                state.show_spinner = false
            }
            state.add_assistant_message(response);
            if let Some(ref timer) = state.timer
                && !state.show_spinner
            {
                timer.cancel.cancel();
                state.timer = None;
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
            state.timer = Some(timer);
            Command::Empty
        }
        Action::InterruptStream => {
            // Stop showing spinner and clear any ongoing streaming
            state.show_spinner = false;
            // Cancel the ongoing stream if one exists
            if let Some(ref cancel) = state.chat_stream {
                cancel.cancel();
                state.chat_stream = None;
            }
            if let Some(ref timer) = state.timer {
                timer.cancel.cancel();
                state.timer = None;
            }
            Command::Empty
        }
        Action::StartStream(cancel_id) => {
            // Store the cancellation token for this stream
            state.chat_stream = Some(cancel_id);
            Command::Empty
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::domain::{CancelId, EditorStateExt};

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
    fn test_update_processes_resize_events() {
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
    fn test_update_processes_mouse_events() {
        let mut fixture_state = State::default();
        let initial_editor_text = fixture_state.editor.get_text();
        let fixture_action =
            Action::CrossTerm(Event::Mouse(ratatui::crossterm::event::MouseEvent {
                kind: ratatui::crossterm::event::MouseEventKind::Down(
                    ratatui::crossterm::event::MouseButton::Left,
                ),
                column: 0,
                row: 0,
                modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
            }));

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
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        fixture_state.timer = Some(timer);

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);

        // Check that cancellation happened automatically and command is Empty
        assert_eq!(actual_command, Command::Empty);
        assert!(cancel_id.is_cancelled());
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
        let cancel_id = CancelId::new(CancellationToken::new());

        let fixture_action = Action::StartStream(cancel_id);

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.chat_stream.is_some());
    }

    #[test]
    fn test_interrupt_stream_action_cancels_stream_token() {
        let mut fixture_state = State::default();
        let cancel_id = CancelId::new(CancellationToken::new());
        fixture_state.chat_stream = Some(cancel_id.clone());
        fixture_state.show_spinner = true;

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);

        // Check that cancellation happened automatically and command is Empty
        assert_eq!(actual_command, Command::Empty);
        assert!(cancel_id.is_cancelled());
        assert!(!fixture_state.show_spinner);
        assert!(fixture_state.chat_stream.is_none());
    }

    #[test]
    fn test_initialize_action_returns_read_workspace_command() {
        let mut fixture_state = State::default();

        let actual_command = update(&mut fixture_state, Action::Initialize);
        let expected_command = Command::ReadWorkspace;

        assert_eq!(actual_command, expected_command);
    }

    #[test]
    fn test_workspace_action_updates_state() {
        let mut fixture_state = State::default();

        let actual_command = update(
            &mut fixture_state,
            Action::Workspace {
                current_dir: Some("/test/path".to_string()),
                current_branch: Some("main".to_string()),
            },
        );
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(
            fixture_state.workspace.current_dir,
            Some("/test/path".to_string())
        );
        assert_eq!(
            fixture_state.workspace.current_branch,
            Some("main".to_string())
        );
    }

    #[test]
    fn test_chat_response_stops_spinner_when_complete() {
        let mut fixture_state = State::default();
        fixture_state.show_spinner = true;
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        fixture_state.timer = Some(timer);

        let chat_response =
            forge_api::ChatResponse::TaskMessage { text: "Hello World".to_string(), is_md: false };
        let actual_command = update(&mut fixture_state, Action::ChatResponse(chat_response));

        // Check that cancellation happened automatically and command is Empty
        assert_eq!(actual_command, Command::Empty);
        assert!(cancel_id.is_cancelled());
        assert!(!fixture_state.show_spinner);
        assert_eq!(fixture_state.timer, None);
    }

    #[test]
    fn test_chat_response_stops_spinner_with_empty_text() {
        let mut fixture_state = State::default();
        fixture_state.show_spinner = true;
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        fixture_state.timer = Some(timer.clone());

        let chat_response =
            forge_api::ChatResponse::TaskMessage { text: "Hello".to_string(), is_md: false };
        let actual_command = update(&mut fixture_state, Action::ChatResponse(chat_response));
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(cancel_id.is_cancelled());
        assert!(!fixture_state.show_spinner);
        assert_eq!(fixture_state.timer, None);
    }

    #[test]
    fn test_conversation_initialized_updates_state() {
        let mut fixture_state = State::default();
        let conversation_id = forge_api::ConversationId::generate();

        let actual_command = update(
            &mut fixture_state,
            Action::ConversationInitialized(conversation_id.clone()),
        );
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(
            fixture_state.conversation.conversation_id,
            Some(conversation_id)
        );
        assert!(!fixture_state.conversation.is_first);
    }

    #[test]
    fn test_interval_tick_updates_spinner_and_timer() {
        let mut fixture_state = State::default();
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        fixture_state.timer = Some(timer.clone());

        let actual_command = update(&mut fixture_state, Action::IntervalTick(timer.clone()));
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Timer should be updated to the new timer from the action
        assert_eq!(fixture_state.timer, Some(timer));
    }

    #[test]
    fn test_interval_tick_replaces_existing_timer() {
        let mut fixture_state = State::default();
        let cancel_id_1 = crate::domain::CancelId::new(CancellationToken::new());
        let timer_1 = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id_1,
        };
        fixture_state.timer = Some(timer_1);

        // Create a different timer for the tick
        let cancel_id_2 = crate::domain::CancelId::new(CancellationToken::new());
        let timer_2 = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id_2,
        };

        let actual_command = update(&mut fixture_state, Action::IntervalTick(timer_2.clone()));
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Timer should be replaced with the new timer from the action
        assert_eq!(fixture_state.timer, Some(timer_2));
    }
}
