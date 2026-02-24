use std::sync::Arc;

use forge_api::Environment;
use nu_ansi_term::{Color, Style};
use reedline::{
    ColumnarMenu, DefaultHinter, EditCommand, Emacs, FileBackedHistory, Highlighter, KeyCode,
    KeyModifiers, MenuBuilder, Prompt, Reedline, ReedlineEvent, ReedlineMenu, Signal, StyledText,
    default_emacs_keybindings,
};

use super::completer::InputCompleter;
use crate::model::ForgeCommandManager;

// TODO: Store the last `HISTORY_CAPACITY` commands in the history file
const HISTORY_CAPACITY: usize = 1024 * 1024;
const COMPLETION_MENU: &str = "completion_menu";

/// Custom highlighter that sets input text color based on terminal theme.
struct ThemeAwareHighlighter;

impl Highlighter for ThemeAwareHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let is_light_mode = matches!(
            terminal_colorsaurus::theme_mode(terminal_colorsaurus::QueryOptions::default()),
            Ok(terminal_colorsaurus::ThemeMode::Light)
        );

        let style = if is_light_mode {
            // Use black for light mode
            nu_ansi_term::Style::new().fg(Color::Black)
        } else {
            // Use default for dark mode (no styling)
            nu_ansi_term::Style::new()
        };

        let mut styled_text = StyledText::new();
        styled_text.push((style, line.to_string()));
        styled_text
    }
}

pub struct ForgeEditor {
    editor: Reedline,
}

pub enum ReadResult {
    Success(String),
    Empty,
    Continue,
    Exit,
}

impl ForgeEditor {
    fn init() -> reedline::Keybindings {
        let mut keybindings = default_emacs_keybindings();
        // on TAB press shows the completion menu, and if we've exact match it will
        // insert it
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::Menu(COMPLETION_MENU.to_string()),
                ReedlineEvent::Edit(vec![EditCommand::Complete]),
            ]),
        );

        // on CTRL + k press clears the screen
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('k'),
            ReedlineEvent::ClearScreen,
        );

        // on CTRL + r press searches the history
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('r'),
            ReedlineEvent::SearchHistory,
        );

        // on ALT + Enter press inserts a newline
        keybindings.add_binding(
            KeyModifiers::ALT,
            KeyCode::Enter,
            ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
        );

        keybindings
    }

    pub fn new(env: Environment, manager: Arc<ForgeCommandManager>) -> Self {
        // Detect terminal theme for appropriate colors
        let is_light_mode = matches!(
            terminal_colorsaurus::theme_mode(terminal_colorsaurus::QueryOptions::default()),
            Ok(terminal_colorsaurus::ThemeMode::Light)
        );

        // Store file history in system config directory
        let history_file = env.history_path();

        let history = Box::new(
            FileBackedHistory::with_file(HISTORY_CAPACITY, history_file).unwrap_or_default(),
        );
        let completion_menu = Box::new(
            ColumnarMenu::default()
                .with_name(COMPLETION_MENU)
                .with_marker("")
                .with_text_style(if is_light_mode {
                    Style::new().bold().fg(Color::Blue)
                } else {
                    Style::new().bold().fg(Color::Cyan)
                })
                .with_selected_text_style(Style::new().on(Color::White).fg(Color::Black)),
        );

        let edit_mode = Box::new(Emacs::new(Self::init()));

        let editor = Reedline::create()
            .with_completer(Box::new(InputCompleter::new(env.cwd, manager)))
            .with_history(history)
            .with_hinter(Box::new(
                DefaultHinter::default().with_style(Style::new().fg(Color::DarkGray)),
            ))
            .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(edit_mode)
            .with_quick_completions(true)
            .with_ansi_colors(true)
            .with_highlighter(Box::new(ThemeAwareHighlighter))
            .use_bracketed_paste(true);
        Self { editor }
    }

    pub fn prompt(&mut self, prompt: &dyn Prompt) -> anyhow::Result<ReadResult> {
        let signal = self.editor.read_line(prompt);
        signal.map(Into::into).map_err(|e| anyhow::anyhow!(e))
    }

    /// Sets the buffer content to be pre-filled on the next prompt
    pub fn set_buffer(&mut self, content: String) {
        self.editor
            .run_edit_commands(&[EditCommand::InsertString(content)]);
    }
}

impl From<Signal> for ReadResult {
    fn from(signal: Signal) -> Self {
        match signal {
            Signal::Success(buffer) => {
                let trimmed = buffer.trim();
                if trimmed.is_empty() {
                    ReadResult::Empty
                } else {
                    ReadResult::Success(trimmed.to_string())
                }
            }
            Signal::CtrlC => ReadResult::Continue,
            Signal::CtrlD => ReadResult::Exit,
        }
    }
}
