use std::time::Duration;

use edtui::actions::{
    Execute, MoveToEndOfLine, MoveToStartOfLine, MoveWordBackward, MoveWordForward,
};
use edtui::{EditorEventHandler, EditorMode};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use strum::IntoEnumIterator;

use crate::domain::{Command, EditorStateExt, SlashCommand, State};

fn handle_slash_menu_navigation(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Option<Command> {
    use ratatui::crossterm::event::KeyCode;

    if !state.slash_menu_visible() {
        return None;
    }

    // Get the current search term (everything after "/")
    let text = state.editor.get_text();
    let search_term = text.strip_prefix('/').unwrap_or("");

    // Get filtered commands using fuzzy search
    let filtered_commands = crate::domain::SlashCommand::fuzzy_filter(search_term);

    match key_event.code {
        KeyCode::Up => {
            let current = state.menu.list.selected().unwrap_or(0);
            if current > 0 {
                state.menu.list.select(Some(current - 1));
            }
            Some(Command::Empty)
        }
        KeyCode::Down => {
            let current = state.menu.list.selected().unwrap_or(0);
            if !filtered_commands.is_empty() && current < filtered_commands.len() - 1 {
                state.menu.list.select(Some(current + 1));
            }
            Some(Command::Empty)
        }
        KeyCode::Enter => {
            // Execute the selected command from filtered results
            if let Some(selected_index) = state.menu.list.selected()
                && let Some(selected_cmd) = filtered_commands.get(selected_index)
            {
                // Replace the current text with the selected command
                state
                    .editor
                    .set_text_insert_mode(format!("/{selected_cmd}"));

                // Convert SlashCommand to appropriate Command for execution
                let command = match selected_cmd {
                    crate::domain::SlashCommand::Exit => Command::Exit,
                    crate::domain::SlashCommand::Agent => {
                        // For now, just return empty - proper agent selection would need more
                        // UI
                        Command::Empty
                    }
                    crate::domain::SlashCommand::Model => {
                        // For now, just return empty - proper model selection would need more
                        // UI
                        Command::Empty
                    }
                    _ => {
                        // For other commands, just return empty for now
                        Command::Empty
                    }
                };

                return Some(command);
            }
            Some(Command::Empty)
        }
        KeyCode::Esc => {
            // Clear the "/" from editor
            state.editor.clear();
            Some(Command::Empty)
        }
        KeyCode::Backspace => {
            // Reset selection when search term changes if menu is still visible
            if state.slash_menu_visible() {
                state.menu.list.select(Some(0));
            }
            None // Let the editor handle the backspace
        }
        _ => {
            // For any other character input, reset selection to first item
            // This will be handled after the editor processes the key
            None
        }
    }
}

fn handle_word_navigation(
    editor: &mut edtui::EditorState,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> bool {
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    if key_event.modifiers.contains(KeyModifiers::ALT) {
        match key_event.code {
            KeyCode::Char('b') => {
                MoveWordBackward(1).execute(editor);
                true
            }
            KeyCode::Char('f') => {
                MoveWordForward(1).execute(editor);
                true
            }
            _ => false,
        }
    } else {
        false
    }
}

fn handle_slash_menu_search_update(state: &mut State) {
    if state.slash_menu_visible() {
        // Reset selection to first item when search term changes
        state.menu.list.select(Some(0));
    }
}

fn handle_line_navigation(
    editor: &mut edtui::EditorState,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> bool {
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    if key_event.modifiers.contains(KeyModifiers::CONTROL) {
        match key_event.code {
            KeyCode::Char('a') => {
                MoveToStartOfLine().execute(editor);
                true
            }
            KeyCode::Char('e') => {
                MoveToEndOfLine().execute(editor);
                true
            }
            _ => false,
        }
    } else {
        false
    }
}

fn handle_prompt_submit(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Command {
    use ratatui::crossterm::event::KeyCode;

    if key_event.code == KeyCode::Enter && state.editor.mode == EditorMode::Normal {
        let message = state.take_lines().join("\n");
        if message.trim().is_empty() {
            Command::Empty
        } else {
            state.add_user_message(message.clone());
            state.show_spinner = true;
            let chat_command = Command::ChatMessage {
                message,
                conversation_id: state.conversation.conversation_id,
                is_first: state.conversation.is_first,
            };
            Command::Interval { duration: Duration::from_millis(100) }.and(chat_command)
        }
    } else {
        Command::Empty
    }
}

fn handle_slash_show(state: &mut State, key_event: ratatui::crossterm::event::KeyEvent) -> Command {
    use ratatui::crossterm::event::KeyCode;

    if key_event.code == KeyCode::Char('/') && state.editor.mode == EditorMode::Insert {
        // Check if we just typed "/" to potentially reset menu selection
        let text = state.editor.get_text();

        // If text is exactly "/" and menu is now visible, reset selection
        if text == "/" && state.slash_menu_visible() {
            // Reset menu selection to the first item
            state.menu.list.select(Some(0));
        }
        Command::Empty
    } else {
        Command::Empty
    }
}

fn handle_menu_navigation(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> bool {
    use ratatui::crossterm::event::KeyCode;

    // Only handle menu navigation when editor is in normal mode
    if state.editor.mode != EditorMode::Normal {
        return false;
    }

    // Get menu items count dynamically to ensure consistency
    let menu_items_count = SlashCommand::iter().count();

    match key_event.code {
        KeyCode::Up => {
            let current_selected = state.menu.list.selected().unwrap_or(0);
            if current_selected > 0 {
                state.menu.list.select(Some(current_selected - 1));
            } else {
                // Wrap to bottom when at top
                state.menu.list.select(Some(menu_items_count - 1));
            }
            true
        }
        KeyCode::Down => {
            let current_selected = state.menu.list.selected().unwrap_or(0);
            if current_selected < menu_items_count - 1 {
                state.menu.list.select(Some(current_selected + 1));
            } else {
                // Wrap to top when at bottom
                state.menu.list.select(Some(0));
            }
            true
        }
        _ => false,
    }
}

fn handle_message_scroll(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> bool {
    use ratatui::crossterm::event::KeyCode;

    // Only handle message scroll when editor is in normal mode
    if state.editor.mode != EditorMode::Normal {
        return false;
    }

    // Check if there are no messages to scroll (menu should be visible and take
    // precedence)
    if state.messages.is_empty() {
        return false;
    }

    match key_event.code {
        KeyCode::Up => {
            state.message_scroll_state.scroll_up();
            true
        }
        KeyCode::Down => {
            state.message_scroll_state.scroll_down();
            true
        }
        _ => false,
    }
}

fn handle_editor_default(
    editor: &mut edtui::EditorState,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Command {
    EditorEventHandler::default().on_key_event(key_event, editor);
    Command::Empty
}

pub fn handle_key_event(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Command {
    // Always handle exit
    if key_event.code == KeyCode::Char('d') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
        return Command::Exit;
    }

    // Handle Ctrl+C interrupt (stop current LLM output stream)
    if key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
        return Command::InterruptStream;
    }

    // Handle slash menu navigation first if visible
    if let Some(slash_cmd) = handle_slash_menu_navigation(state, key_event) {
        // Update menu visibility after handling slash menu navigation
        state.update_menu_visibility();
        return slash_cmd;
    }

    // Handle menu navigation first (only in normal mode when no messages or menu
    // takes precedence)
    let menu_nav_handled = handle_menu_navigation(state, key_event);
    if menu_nav_handled {
        return Command::Empty;
    }

    // Handle message scrolling second (only in normal mode when menu doesn't handle
    // it)
    let scroll_cmd = handle_message_scroll(state, key_event);
    if scroll_cmd {
        return Command::Empty;
    }

    // Check if navigation was handled first
    let line_nav_handled = handle_line_navigation(&mut state.editor, key_event);
    let word_nav_handled = handle_word_navigation(&mut state.editor, key_event);

    // Only call editor default and slash show if no navigation was handled
    if !line_nav_handled && !word_nav_handled {
        let result = handle_editor_default(&mut state.editor, key_event)
            .and(handle_slash_show(state, key_event))
            .and(handle_prompt_submit(state, key_event));

        // Update slash menu search if menu is visible
        handle_slash_menu_search_update(state);

        // Update menu visibility after any editor changes
        state.update_menu_visibility();

        result
    } else {
        Command::Empty
    }
}

#[cfg(test)]
mod tests {
    use edtui::Index2;
    use pretty_assertions::assert_eq;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::domain::State;

    fn create_test_state_with_text() -> State {
        let mut state = State::default();
        // Set up some text content for testing cursor movement
        state.editor.set_text_with_cursor_at_end(
            "hello world this is a test\nsecond line here".to_string(),
        );
        // Position cursor in the middle of the first word for testing
        state.editor.cursor = Index2::new(0, 6); // After "hello "
        state
    }

    #[test]
    fn test_macos_option_left_moves_word_backward() {
        let mut state = create_test_state_with_text();
        let initial_cursor = state.editor.cursor;
        let key_event = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should have moved backward to the beginning of the previous word
        assert!(state.editor.cursor.col < initial_cursor.col);
    }

    #[test]
    fn test_macos_option_right_moves_word_forward() {
        let mut state = create_test_state_with_text();
        let initial_cursor = state.editor.cursor;
        let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should have moved forward to the beginning of the next word
        assert!(state.editor.cursor.col > initial_cursor.col);
    }

    #[test]
    fn test_macos_cmd_left_moves_to_line_start() {
        let mut state = create_test_state_with_text();
        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should be at the beginning of the line
        assert_eq!(state.editor.cursor.col, 0);
    }

    #[test]
    fn test_macos_cmd_right_moves_to_line_end() {
        let mut state = create_test_state_with_text();
        let initial_row = state.editor.cursor.row;
        let key_event = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should be at the end of the current line
        // The first line is "hello world this is a test" (25 characters, 0-indexed so
        // position 25)
        assert_eq!(state.editor.cursor.row, initial_row);
        assert_eq!(state.editor.cursor.col, 25);
    }

    #[test]
    fn test_regular_arrow_keys_still_work() {
        let mut state = create_test_state_with_text();
        let _initial_cursor = state.editor.cursor;
        let key_event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Regular arrow keys should pass through to the editor
        // The cursor position might change due to normal editor handling
        // We just verify the command was processed normally
    }

    #[test]
    fn test_exit_command_works() {
        let mut state = create_test_state_with_text();
        let key_event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Exit;

        assert_eq!(actual_command, expected_command);
    }

    #[test]
    fn test_ctrl_c_interrupt_stops_stream() {
        let mut state = create_test_state_with_text();
        let key_event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::InterruptStream;

        assert_eq!(actual_command, expected_command);
    }

    #[test]
    fn test_navigation_prevents_editor_default_and_slash_show() {
        let mut state = create_test_state_with_text();
        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);

        // Navigation handling should short-circuit other calls
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should have moved to line start (navigation was handled)
        assert_eq!(state.editor.cursor.col, 0);
    }

    #[test]
    fn test_word_navigation_prevents_editor_default_and_slash_show() {
        let mut state = create_test_state_with_text();
        let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);

        // Word navigation handling should short-circuit other calls
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should have moved forward (navigation was handled)
        assert!(state.editor.cursor.col > 6); // Started at position 6
    }

    #[test]
    fn test_slash_auto_detection_shows_slash_menu() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;

        // Simulate typing "/" in insert mode
        let key_event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Slash menu should be visible after typing "/"
        assert!(state.slash_menu_visible());
        // Main editor should have the "/" character
        assert_eq!(state.editor.get_text(), "/");
        // Menu should be selected to first item
        assert_eq!(state.menu.list.selected(), Some(0));
    }

    #[test]
    fn test_slash_auto_detection_only_triggers_on_exact_slash() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;

        // Set text to something that starts with "/" but is not exactly "/"
        state.editor.set_text_insert_mode("/test".to_string());

        let key_event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Slash menu should still be visible since text starts with "/"
        assert!(state.slash_menu_visible());
        // Main editor should have the new character added
        assert_eq!(state.editor.get_text(), "/testx");
    }

    #[test]
    fn test_handle_prompt_submit_with_empty_input() {
        let mut fixture = State::default();
        fixture.editor.mode = EditorMode::Normal;
        fixture.editor.clear();

        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

        let actual = handle_prompt_submit(&mut fixture, key_event);
        let expected = Command::Empty;

        assert_eq!(actual, expected);
        assert_eq!(fixture.messages.len(), 0);
        assert!(!fixture.show_spinner);
    }

    #[test]
    fn test_menu_navigation_comprehensive() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Normal;

        // Test basic down navigation
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        assert_eq!(actual_command, Command::Empty);
        assert_eq!(state.menu.list.selected(), Some(1));

        // Test basic up navigation
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        assert_eq!(actual_command, Command::Empty);
        assert_eq!(state.menu.list.selected(), Some(0));

        // Test wrapping from top to bottom (up from index 0)
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        assert_eq!(actual_command, Command::Empty);
        assert_eq!(state.menu.list.selected(), Some(13));

        // Test wrapping from bottom to top (down from last index)
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        assert_eq!(actual_command, Command::Empty);
        assert_eq!(state.menu.list.selected(), Some(0));
    }

    #[test]
    fn test_menu_navigation_disabled_states() {
        let mut state = State::default();

        // Test navigation disabled when editor is in insert mode
        state.editor.mode = EditorMode::Insert;
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        assert_eq!(actual_command, Command::Empty);
        assert_eq!(state.menu.list.selected(), Some(0)); // Should not change
    }

    #[test]
    fn test_menu_navigation_works_with_messages() {
        let mut state = State::default();

        state.editor.mode = EditorMode::Normal;
        state.add_user_message("Test message".to_string()); // Add messages to state

        // Test that menu navigation still works when messages are present
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        assert_eq!(actual_command, Command::Empty);
        assert_eq!(state.menu.list.selected(), Some(1));
    }

    #[test]
    fn test_slash_menu_navigation_up_down() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/".to_string());
        state.menu.list.select(Some(2));

        // Test down navigation
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(state.menu.list.selected(), Some(3));

        // Test up navigation
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(state.menu.list.selected(), Some(2));
    }

    #[test]
    fn test_slash_menu_enter_executes_command() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/exit".to_string());
        state.menu.list.select(Some(0)); // Select first item which should be "exit"

        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Exit;

        assert_eq!(actual_command, expected_command);
        assert!(state.slash_menu_visible()); // Menu should still be visible since text is "/exit"
        assert_eq!(state.editor.get_text(), "/exit"); // Text should be updated
    }

    #[test]
    fn test_slash_menu_escape_hides_menu() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/test".to_string());

        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(!state.slash_menu_visible()); // Menu should not be visible after clearing text
        assert_eq!(state.editor.get_text(), ""); // Text should be cleared
    }

    #[test]
    fn test_slash_menu_backspace_behavior() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/test".to_string());

        // Position cursor at end for backspace
        state.editor.cursor = Index2::new(0, 5);

        let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let _actual_command = handle_key_event(&mut state, key_event);

        // Command should let editor handle the backspace
        // The slash menu should remain visible since text still starts with "/"
        assert!(state.slash_menu_visible());
        assert_eq!(state.editor.get_text(), "/tes");
    }

    #[test]
    fn test_slash_menu_backspace_hides_when_empty() {
        let mut state = State::default();
        state.editor.mode = EditorMode::Insert;
        state.editor.set_text_insert_mode("/".to_string());

        // Position cursor at end for backspace
        state.editor.cursor = Index2::new(0, 1);

        let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        handle_key_event(&mut state, key_event);

        // After backspacing the "/", menu should be hidden
        assert!(!state.slash_menu_visible());
        assert_eq!(state.editor.get_text(), "");
    }
}
