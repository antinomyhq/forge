use std::time::Duration;

use edtui::actions::{
    Execute, MoveToEndOfLine, MoveToStartOfLine, MoveWordBackward, MoveWordForward,
};
use edtui::{EditorEventHandler, EditorMode};
use forge_walker::Walker;
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32Str};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use crate::domain::{Command, EditorStateExt, LayoverState, State};

fn handle_spotlight_input_change(state: &mut State) {
    // Reset selection index when input changes to ensure it's within bounds
    // of the filtered results
    let filtered_count = state.spotlight.filtered_commands().len();

    // Reset selection to 0 if current selection is out of bounds
    if state.spotlight.selected_index >= filtered_count {
        state.spotlight.selected_index = 0;
    }
}

fn handle_spotlight_navigation(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Option<Command> {
    use ratatui::crossterm::event::KeyCode;

    if !state.layover_state.is_spotlight() {
        return None;
    }

    let filtered_commands = state.spotlight.filtered_commands();

    match key_event.code {
        KeyCode::Up => {
            if state.spotlight.selected_index > 0 {
                state.spotlight.selected_index -= 1;
            }
            Some(Command::Empty)
        }
        KeyCode::Down => {
            // Use filtered commands count for navigation
            let max_commands = filtered_commands.len();
            if max_commands > 0 && state.spotlight.selected_index < max_commands - 1 {
                state.spotlight.selected_index += 1;
            }
            Some(Command::Empty)
        }
        KeyCode::Tab => {
            // Auto-complete with the first matching command
            if !filtered_commands.is_empty() {
                let first_match = filtered_commands[0].to_string();
                // Clear current input and set to the first match
                state.spotlight.editor.set_text_insert_mode(first_match);
                state.spotlight.selected_index = 0;
            }
            Some(Command::Empty)
        }
        KeyCode::Enter => {
            // Execute the selected command
            if let Some(selected_cmd) = state.spotlight.selected_command() {
                // Convert SlashCommand to appropriate Command
                let command = match selected_cmd {
                    crate::domain::slash_command::SlashCommand::Exit => Command::Exit,
                    crate::domain::slash_command::SlashCommand::Agent => {
                        // For now, just hide spotlight - proper agent selection would need more UI
                        Command::Empty
                    }
                    crate::domain::slash_command::SlashCommand::Model => {
                        // For now, just hide spotlight - proper model selection would need more UI
                        Command::Empty
                    }
                    _ => {
                        // For other commands, just hide spotlight for now
                        Command::Empty
                    }
                };

                // Hide spotlight and return the command
                state.layover_state = LayoverState::Editor;
                return Some(command);
            }
            Some(Command::Empty)
        }
        _ => None,
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

fn handle_spotlight_show(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> Command {
    use ratatui::crossterm::event::KeyCode;

    if key_event.code == KeyCode::Char(':') && state.editor.mode == EditorMode::Normal {
        state.layover_state = LayoverState::Spotlight;
        Command::Empty
    } else {
        Command::Empty
    }
}

fn handle_spotlight_toggle(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
    original_editor_mode: EditorMode,
) -> Command {
    use ratatui::crossterm::event::KeyCode;

    if key_event.code == KeyCode::Esc {
        if !state.layover_state.is_spotlight() && original_editor_mode == EditorMode::Normal {
            // Open spotlight when it's closed and editor was originally in normal mode
            state.layover_state = LayoverState::Spotlight;
        } else {
            // Hide spotlight in all other cases
            state.layover_state = LayoverState::Editor;
        }
        Command::Empty
    } else {
        Command::Empty
    }
}

fn handle_message_scroll(
    state: &mut State,
    key_event: ratatui::crossterm::event::KeyEvent,
) -> bool {
    use ratatui::crossterm::event::KeyCode;

    if state.layover_state.is_spotlight() || state.editor.mode != EditorMode::Normal {
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
    // Always handle exit regardless of spotlight state
    if key_event.code == KeyCode::Char('d') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
        return Command::Exit;
    }

    // Handle Ctrl+C interrupt (stop current LLM output stream)
    if key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
        return Command::InterruptStream;
    }

    if key_event.code == KeyCode::Tab
        && state.editor.mode == EditorMode::Insert
        && state.editor.get_text_from_at_to_cursor().is_some()
    {
        return Command::Autocomplete;
    }

    if state.layover_state.is_spotlight() {
        // When spotlight is visible, route events to spotlight editor
        let cmd = handle_spotlight_toggle(state, key_event, state.editor.mode);

        // Check spotlight navigation first
        let spotlight_nav_cmd = handle_spotlight_navigation(state, key_event);

        if spotlight_nav_cmd.is_none() {
            // Check if navigation was handled
            let line_nav_handled = handle_line_navigation(&mut state.spotlight.editor, key_event);
            let word_nav_handled = handle_word_navigation(&mut state.spotlight.editor, key_event);

            // Only call editor default if no navigation was handled
            let result_cmd = if !line_nav_handled && !word_nav_handled {
                let editor_cmd = handle_editor_default(&mut state.spotlight.editor, key_event);
                // Reset selection index when input changes
                handle_spotlight_input_change(state);
                cmd.and(editor_cmd)
            } else {
                cmd
            };

            // Always keep spotlight in "insert" mode
            state.spotlight.editor.mode = EditorMode::Insert;
            result_cmd
        } else {
            // Spotlight navigation handled, return the command from navigation
            cmd.and(spotlight_nav_cmd.unwrap_or(Command::Empty))
        }
    } else if let LayoverState::Autocomplete(ref mut autocomplete_state) = state.layover_state {
        use ratatui::crossterm::event::KeyCode;
        let suggestions_len = autocomplete_state.suggestions.len();
        let selected = autocomplete_state.list_state.selected().unwrap_or(0);

        match key_event.code {
            KeyCode::Up if key_event.modifiers.is_empty() => {
                if suggestions_len > 0 && selected > 0 {
                    autocomplete_state.list_state.select(Some(selected - 1));
                    autocomplete_state.selected_index = selected - 1;
                }
                Command::Empty
            }
            KeyCode::Down if key_event.modifiers.is_empty() => {
                if suggestions_len > 0 && selected < suggestions_len - 1 {
                    autocomplete_state.list_state.select(Some(selected + 1));
                    autocomplete_state.selected_index = selected + 1;
                }
                Command::Empty
            }
            KeyCode::Tab | KeyCode::Enter => {
                // Insert the selected suggestion into the editor
                if suggestions_len > 0 {
                    let suggestion = &autocomplete_state.suggestions[selected];
                    // Replace the text from @ to cursor with the suggestion
                    if let Some(text) = state.editor.get_text_from_at_to_cursor() {
                        let input = state.editor.get_text();
                        if let Some(at_pos) = input.rfind('@') {
                            let before = &input[..at_pos];
                            let after = &input[at_pos + text.len() + 1..]; // +1 for the '@'
                            let new_text = format!("{before}@{suggestion}{after}");
                            state.editor.set_text_insert_mode(new_text);
                        }
                    }
                }
                // Hide autocomplete after selection
                state.layover_state = LayoverState::Editor;
                Command::Empty
            }
            KeyCode::Esc => {
                // Hide autocomplete on escape
                state.layover_state = LayoverState::Editor;
                Command::Empty
            }
            _ => {
                // For all other keys, pass to the autocomplete editor for navigation and
                // editing
                let line_nav_handled =
                    handle_line_navigation(&mut autocomplete_state.editor, key_event);
                let word_nav_handled =
                    handle_word_navigation(&mut autocomplete_state.editor, key_event);

                if !line_nav_handled && !word_nav_handled {
                    EditorEventHandler::default()
                        .on_key_event(key_event, &mut autocomplete_state.editor);
                }

                // Update search term and refilter results
                autocomplete_state.update_search_term();
                let search_term = autocomplete_state.search_term.clone();

                // Refilter files based on updated search term
                if !search_term.is_empty() {
                    let workspace_pathbuf = state.cwd.as_ref().expect("CWD should be set").clone();
                    let walker = Walker::max_all().cwd(workspace_pathbuf).skip_binary(true);
                    let files = walker.get_blocking().unwrap_or_default();
                    let mut fuzzy_matcher = Matcher::new(Config::DEFAULT);
                    let query = search_term.trim();
                    let mut haystack_buf = Vec::new();
                    let mut scored_matches: Vec<(u32, String)> = files
                        .into_iter()
                        .filter_map(|file| {
                            if let Some(file_name) = file.file_name.as_ref() {
                                let haystack = Utf32Str::new(file_name, &mut haystack_buf);
                                let pattern = Pattern::parse(
                                    query,
                                    CaseMatching::Ignore,
                                    Normalization::Smart,
                                );
                                if let Some(score) = pattern.score(haystack, &mut fuzzy_matcher) {
                                    Some((score, file.path.clone()))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect();
                    scored_matches.sort_by(|a, b| b.0.cmp(&a.0));
                    let suggestions: Vec<String> =
                        scored_matches.into_iter().map(|(_, path)| path).collect();

                    autocomplete_state.suggestions = suggestions;

                    // Reset selection if needed
                    if autocomplete_state.suggestions.is_empty() {
                        autocomplete_state.selected_index = 0;
                        autocomplete_state.list_state.select(None);
                    } else if autocomplete_state.selected_index
                        >= autocomplete_state.suggestions.len()
                    {
                        autocomplete_state.selected_index = 0;
                        autocomplete_state.list_state.select(Some(0));
                    }
                } else {
                    // Clear suggestions if search term is empty
                    autocomplete_state.suggestions.clear();
                    autocomplete_state.selected_index = 0;
                    autocomplete_state.list_state.select(None);
                }

                Command::Empty
            }
        }
    } else {
        // When spotlight is not visible, route events to main editor
        // Capture original editor mode before any modifications
        let original_editor_mode = state.editor.mode;

        // Handle message scrolling first (only in normal mode)
        let scroll_cmd = handle_message_scroll(state, key_event);
        if scroll_cmd {
            return Command::Empty;
        }

        // Check if navigation was handled first
        let line_nav_handled = handle_line_navigation(&mut state.editor, key_event);
        let word_nav_handled = handle_word_navigation(&mut state.editor, key_event);

        // Only call editor default and spotlight show if no navigation was handled
        if !line_nav_handled && !word_nav_handled {
            handle_editor_default(&mut state.editor, key_event)
                .and(handle_spotlight_show(state, key_event))
                .and(handle_spotlight_toggle(
                    state,
                    key_event,
                    original_editor_mode,
                ))
                .and(handle_prompt_submit(state, key_event))
        } else {
            Command::Empty
        }
    }
}

#[cfg(test)]
mod tests {
    use edtui::Index2;
    use pretty_assertions::assert_eq;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::domain::State;
    use crate::domain::slash_command::SlashCommand;

    fn create_test_state_with_text() -> State {
        let mut state = State::default();
        // Set up some text content for testing cursor movement
        state.editor.set_text_with_cursor_at_end(
            "hello world this is a test\nsecond line here".to_string(),
        );
        // Position cursor in the middle of the first word for testing
        state.editor.cursor = Index2::new(0, 6); // After "hello "
        // Ensure spotlight is not visible for main editor tests
        state.layover_state = LayoverState::Editor;
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
    fn test_spotlight_visible_routes_events_to_spotlight_editor() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Spotlight;
        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // When spotlight is visible, cursor movement should affect spotlight editor
        assert_eq!(state.spotlight.editor.cursor.col, 0);
        // Main editor cursor should remain unchanged
        assert_eq!(state.editor.cursor.col, 6);
    }

    #[test]
    fn test_spotlight_hidden_routes_events_to_main_editor() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Editor;
        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // When spotlight is hidden, cursor movement should affect main editor
        assert_eq!(state.editor.cursor.col, 0);
        // Spotlight editor cursor should remain unchanged
        assert_eq!(state.spotlight.editor.cursor.col, 0);
    }

    #[test]
    fn test_escape_opens_spotlight_when_closed_and_in_normal_mode() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Editor;
        state.editor.mode = EditorMode::Normal;
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(matches!(state.layover_state, LayoverState::Spotlight));
    }

    #[test]
    fn test_escape_hides_spotlight_when_visible() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Spotlight;
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(matches!(state.layover_state, LayoverState::Editor));
    }

    #[test]
    fn test_escape_does_not_open_spotlight_when_editor_in_insert_mode() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Editor;
        state.editor.mode = EditorMode::Insert;
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert!(matches!(state.layover_state, LayoverState::Editor));
    }

    #[test]
    fn test_exit_command_works_regardless_of_spotlight_state() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Spotlight;
        let key_event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Exit;

        assert_eq!(actual_command, expected_command);
    }

    #[test]
    fn test_ctrl_c_interrupt_stops_stream_regardless_of_spotlight_state() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Spotlight;
        let key_event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::InterruptStream;

        assert_eq!(actual_command, expected_command);
    }

    #[test]
    fn test_ctrl_c_interrupt_stops_stream_when_spotlight_hidden() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Editor;
        let key_event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::InterruptStream;

        assert_eq!(actual_command, expected_command);
    }

    #[test]
    fn test_spotlight_word_navigation() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Spotlight;
        // Set up some text in spotlight editor
        state
            .spotlight
            .editor
            .set_text_with_cursor_at_end("hello world test".to_string());
        state.spotlight.editor.cursor = Index2::new(0, 6); // After "hello "
        let initial_cursor = state.spotlight.editor.cursor;
        let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);

        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should have moved forward in spotlight editor
        assert!(state.spotlight.editor.cursor.col > initial_cursor.col);
    }

    #[test]
    fn test_navigation_prevents_editor_default_and_spotlight_show() {
        let mut state = create_test_state_with_text();
        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);

        // Before the fix, this would have called editor_default and potentially
        // spotlight_show After the fix, navigation handling should
        // short-circuit these calls
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should have moved to line start (navigation was handled)
        assert_eq!(state.editor.cursor.col, 0);
        // Spotlight should remain hidden (spotlight_show was not called)
        assert!(!state.layover_state.is_spotlight());
    }

    #[test]
    fn test_word_navigation_prevents_editor_default_and_spotlight_show() {
        let mut state = create_test_state_with_text();
        let key_event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);

        // Before the fix, this would have called editor_default and potentially
        // spotlight_show After the fix, word navigation handling should
        // short-circuit these calls
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        // Cursor should have moved forward (navigation was handled)
        assert!(state.editor.cursor.col > 6); // Started at position 6
        // Spotlight should remain hidden (spotlight_show was not called)
        assert!(!state.layover_state.is_spotlight());
    }

    #[test]
    fn test_spotlight_navigation_up_down() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Spotlight;
        state.spotlight.selected_index = 2;

        // Test down navigation
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(state.spotlight.selected_index, 3);

        // Test up navigation
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(state.spotlight.selected_index, 2);
    }

    #[test]
    fn test_spotlight_navigation_boundaries() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Spotlight;
        state.spotlight.selected_index = 0;

        // Test up navigation at top boundary
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(state.spotlight.selected_index, 0); // Should stay at 0

        // Move to bottom
        state.spotlight.selected_index = 14; // Max index for 15 commands

        // Test down navigation at bottom boundary
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(state.spotlight.selected_index, 14); // Should stay at 14
    }

    #[test]
    fn test_spotlight_navigation_when_not_visible() {
        let mut state = create_test_state_with_text();
        state.layover_state = LayoverState::Editor;
        state.spotlight.selected_index = 2;

        // Test that navigation doesn't work when spotlight is not visible
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Empty;

        assert_eq!(actual_command, expected_command);
        assert_eq!(state.spotlight.selected_index, 2); // Should not change
    }

    #[test]
    fn test_spotlight_shows_slash_commands() {
        let mut state = State::default();
        state.layover_state = LayoverState::Spotlight;

        // Test that spotlight shows all slash commands
        let filtered_commands = state.spotlight.filtered_commands();
        assert_eq!(filtered_commands.len(), 12); // All 12 slash commands

        // Test that filtering works
        state
            .spotlight
            .editor
            .set_text_insert_mode("ex".to_string());
        let filtered_commands = state.spotlight.filtered_commands();
        assert_eq!(filtered_commands.len(), 1); // Only "exit" command
        assert_eq!(filtered_commands[0], SlashCommand::Exit);

        // Test selected command
        let selected = state.spotlight.selected_command();
        assert_eq!(selected, Some(SlashCommand::Exit));
    }

    #[test]
    fn test_spotlight_command_execution() {
        let mut state = State::default();
        state.layover_state = LayoverState::Spotlight;

        // Set up to select the exit command
        state
            .spotlight
            .editor
            .set_text_insert_mode("exit".to_string());
        state.spotlight.selected_index = 0;

        // Test Enter key executes the command
        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let actual_command = handle_key_event(&mut state, key_event);
        let expected_command = Command::Exit;

        assert_eq!(actual_command, expected_command);
        // Spotlight should be hidden after command execution
        assert!(matches!(state.layover_state, LayoverState::Editor));
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
}
