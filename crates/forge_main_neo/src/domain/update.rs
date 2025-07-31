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
                state.spinner = None;
            }
            state.add_assistant_message(response);
            // Timer is managed by the spinner, so no need to handle it separately here
            Command::Empty
        }
        Action::ConversationInitialized(conversation_id) => {
            state.conversation.init_conversation(conversation_id);
            Command::Empty
        }
        Action::IntervalTick(timer) => {
            if let Some(ref mut spinner) = state.spinner {
                spinner.throbber.calc_next();
                spinner.timer = Some(timer);
            }
            // For now, interval ticks don't trigger any state changes or commands
            // This could be extended to update a timer display or trigger other actions
            Command::Empty
        }
        Action::InterruptStream => {
            state.spinner = None;
            // Cancel the ongoing stream if one exists
            if let Some(ref cancel) = state.chat_stream {
                cancel.cancel();
                state.chat_stream = None;
            }
            Command::Empty
        }
        Action::StartStream(cancel_id) => {
            // Store the cancellation token for this stream
            state.chat_stream = Some(cancel_id);
            Command::Empty
        }
        Action::CompactionResult(compaction_result) => {
            state.spinner = None;

            match compaction_result {
                Ok(result) => {
                    let token_reduction = result.token_reduction_percentage();
                    let message_reduction = result.message_reduction_percentage();

                    let message = format!(
                        "Compaction complete: {token_reduction:.1}% token reduction, {message_reduction:.1}% message reduction"
                    );
                    state.add_user_message(message);
                }
                Err(err) => {
                    let message = format!("Compaction failed: {err}");
                    state.add_user_message(message);
                }
            };
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
    use crate::domain::{CancelId, EditorStateExt, Message};

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
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        let mut spinner = crate::domain::state::Spinner::new();
        spinner.timer = Some(timer);
        fixture_state.spinner = Some(spinner);

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);

        // Check that cancellation happened automatically and command is Empty
        assert_eq!(actual_command, Command::Empty);
        assert!(cancel_id.is_cancelled());
        assert!(fixture_state.spinner.is_none());
    }

    #[test]
    fn test_interrupt_stream_action_when_no_timer_active() {
        let mut fixture_state = State::default();
        fixture_state.spinner = Some(crate::domain::state::Spinner::new());

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.spinner.is_none());
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
        fixture_state.spinner = Some(crate::domain::state::Spinner::new());

        let fixture_action = Action::InterruptStream;

        let actual_command = update(&mut fixture_state, fixture_action);

        // Check that cancellation happened automatically and command is Empty
        assert_eq!(actual_command, Command::Empty);
        assert!(cancel_id.is_cancelled());
        assert!(fixture_state.spinner.is_none());
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
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        let mut spinner = crate::domain::state::Spinner::new();
        spinner.timer = Some(timer);
        fixture_state.spinner = Some(spinner);

        let chat_response = forge_api::ChatResponse::Text {
            text: "Hello World".to_string(),
            is_complete: true,
            is_md: false,
        };
        let actual_command = update(&mut fixture_state, Action::ChatResponse(chat_response));

        // Check that cancellation happened automatically and command is Empty
        assert_eq!(actual_command, Command::Empty);
        assert!(cancel_id.is_cancelled());
        assert!(fixture_state.spinner.is_none());
    }

    #[test]
    fn test_chat_response_continues_spinner_when_streaming() {
        let mut fixture_state = State::default();
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        let mut spinner = crate::domain::state::Spinner::new();
        spinner.timer = Some(timer.clone());
        fixture_state.spinner = Some(spinner);

        let chat_response = forge_api::ChatResponse::Text {
            text: "Hello".to_string(),
            is_complete: false,
            is_md: false,
        };
        let actual_command = update(&mut fixture_state, Action::ChatResponse(chat_response));
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.spinner.is_some());
        // Timer should still be present in the spinner
        assert!(fixture_state.spinner.as_ref().unwrap().timer.is_some());
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
        let mut spinner = crate::domain::state::Spinner::new();
        spinner.timer = Some(timer.clone());
        fixture_state.spinner = Some(spinner);

        // Create a new timer with updated current_time
        let updated_timer = crate::domain::state::Timer {
            start_time: timer.start_time,
            current_time: chrono::Utc::now() + chrono::Duration::seconds(1),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };

        let actual_command = update(
            &mut fixture_state,
            Action::IntervalTick(updated_timer.clone()),
        );
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Timer should be updated with new current_time, but keep original start_time
        let spinner_timer = fixture_state
            .spinner
            .as_ref()
            .unwrap()
            .timer
            .as_ref()
            .unwrap();
        assert_eq!(spinner_timer.current_time, updated_timer.current_time);
        assert_eq!(spinner_timer.start_time, timer.start_time); // Original start_time preserved
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
        let mut spinner = crate::domain::state::Spinner::new();
        spinner.timer = Some(timer_1.clone());
        fixture_state.spinner = Some(spinner);

        // Create a different timer for the tick - this represents a new tick with
        // updated time
        let cancel_id_2 = crate::domain::CancelId::new(CancellationToken::new());
        let timer_2 = crate::domain::state::Timer {
            start_time: timer_1.start_time, // Same operation, so same start time
            current_time: chrono::Utc::now() + chrono::Duration::seconds(2),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id_2,
        };

        let actual_command = update(&mut fixture_state, Action::IntervalTick(timer_2.clone()));
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Timer current_time should be updated while preserving original start_time
        let spinner_timer = fixture_state
            .spinner
            .as_ref()
            .unwrap()
            .timer
            .as_ref()
            .unwrap();
        assert_eq!(spinner_timer.current_time, timer_2.current_time);
        assert_eq!(spinner_timer.start_time, timer_1.start_time); // Original start_time preserved
    }

    #[test]
    fn test_compaction_result_success_stops_spinner_and_adds_message() {
        let mut fixture_state = State::default();
        fixture_state.spinner =
            Some(crate::domain::state::Spinner::new().message(Some("Compacting".to_string())));

        let fixture_compaction_result = forge_api::CompactionResult::new(1000, 500, 20, 10);
        let fixture_action = Action::CompactionResult(Ok(fixture_compaction_result));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.spinner.is_none());

        // Check that success message was added
        let actual_messages = &fixture_state.messages;
        assert_eq!(actual_messages.len(), 1);
        let expected_message_content =
            "Compaction complete: 50.0% token reduction, 50.0% message reduction";
        match &actual_messages[0] {
            Message::User(content) => assert!(content.contains(expected_message_content)),
            _ => panic!("Expected User message"),
        }
    }

    #[test]
    fn test_compaction_result_success_cancels_timer() {
        let mut fixture_state = State::default();
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        let mut spinner = crate::domain::state::Spinner::new();
        spinner.timer = Some(timer);
        fixture_state.spinner = Some(spinner);

        let fixture_compaction_result = forge_api::CompactionResult::new(1000, 500, 20, 10);
        let fixture_action = Action::CompactionResult(Ok(fixture_compaction_result));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.spinner.is_none());
        assert!(cancel_id.is_cancelled());
    }

    #[test]
    fn test_compaction_result_error_stops_spinner_and_adds_error_message() {
        let mut fixture_state = State::default();
        fixture_state.spinner =
            Some(crate::domain::state::Spinner::new().message(Some("Compacting".to_string())));

        let fixture_error_message = "Compaction failed due to network error".to_string();
        let fixture_action =
            Action::CompactionResult(Err(anyhow::anyhow!(fixture_error_message.clone())));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.spinner.is_none());

        // Check that error message was added
        let actual_messages = &fixture_state.messages;
        assert_eq!(actual_messages.len(), 1);
        let expected_message_content = format!("Compaction failed: {}", fixture_error_message);
        match &actual_messages[0] {
            Message::User(content) => assert!(content.contains(&expected_message_content)),
            _ => panic!("Expected User message"),
        }
    }

    #[test]
    fn test_compaction_result_error_cancels_timer() {
        let mut fixture_state = State::default();
        let cancel_id = crate::domain::CancelId::new(CancellationToken::new());
        let timer = crate::domain::state::Timer {
            start_time: chrono::Utc::now(),
            current_time: chrono::Utc::now(),
            duration: std::time::Duration::from_secs(1),
            cancel: cancel_id.clone(),
        };
        let mut spinner = crate::domain::state::Spinner::new();
        spinner.timer = Some(timer);
        fixture_state.spinner = Some(spinner);

        let fixture_error_message = "Compaction failed".to_string();
        let fixture_action = Action::CompactionResult(Err(anyhow::anyhow!(fixture_error_message)));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.spinner.is_none());
        assert!(cancel_id.is_cancelled());
    }

    #[test]
    fn test_compaction_result_success_with_zero_reduction() {
        let mut fixture_state = State::default();

        let fixture_compaction_result = forge_api::CompactionResult::new(1000, 1000, 20, 20);
        let fixture_action = Action::CompactionResult(Ok(fixture_compaction_result));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);

        // Check that message shows 0% reduction
        let actual_messages = &fixture_state.messages;
        assert_eq!(actual_messages.len(), 1);
        let expected_message_content =
            "Compaction complete: 0.0% token reduction, 0.0% message reduction";
        match &actual_messages[0] {
            Message::User(content) => assert!(content.contains(expected_message_content)),
            _ => panic!("Expected User message"),
        }
    }

    #[test]
    fn test_compaction_result_success_with_complete_reduction() {
        let mut fixture_state = State::default();

        let fixture_compaction_result = forge_api::CompactionResult::new(1000, 0, 20, 0);
        let fixture_action = Action::CompactionResult(Ok(fixture_compaction_result));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);

        // Check that message shows 100% reduction
        let actual_messages = &fixture_state.messages;
        assert_eq!(actual_messages.len(), 1);
        let expected_message_content =
            "Compaction complete: 100.0% token reduction, 100.0% message reduction";
        match &actual_messages[0] {
            Message::User(content) => assert!(content.contains(expected_message_content)),
            _ => panic!("Expected User message"),
        }
    }

    #[test]
    fn test_compaction_result_success_when_no_timer_active() {
        let mut fixture_state = State::default();
        fixture_state.spinner = Some(crate::domain::state::Spinner::new());

        let fixture_compaction_result = forge_api::CompactionResult::new(1000, 500, 20, 10);
        let fixture_action = Action::CompactionResult(Ok(fixture_compaction_result));

        let actual_command = update(&mut fixture_state, fixture_action);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(fixture_state.spinner.is_none());

        // Should still add success message
        let actual_messages = &fixture_state.messages;
        assert_eq!(actual_messages.len(), 1);
    }
}
