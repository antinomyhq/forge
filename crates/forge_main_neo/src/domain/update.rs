use edtui::EditorEventHandler;
use forge_api::ChatResponse;

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
            ratatui::crossterm::event::Event::Key(key_event) => handle_key_event(state, key_event),
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
    }
}
