use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossterm::event::Event;
use forge_api::Environment;
use nu_ansi_term::{Color, Style};
use reedline::{
    ColumnarMenu, DefaultHinter, EditCommand, EditMode, Emacs, FileBackedHistory, KeyCode,
    KeyModifiers, MenuBuilder, Prompt, PromptEditMode, Reedline, ReedlineEvent, ReedlineMenu,
    ReedlineRawEvent, Signal, default_emacs_keybindings,
};

use super::completer::InputCompleter;
use crate::model::ForgeCommandManager;

// TODO: Store the last `HISTORY_CAPACITY` commands in the history file
const HISTORY_CAPACITY: usize = 1024 * 1024;
const COMPLETION_MENU: &str = "completion_menu";

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

    pub fn new(
        env: Environment,
        custom_history_path: Option<PathBuf>,
        manager: Arc<ForgeCommandManager>,
    ) -> Self {
        // Store file history in system config directory
        let history_file = env.history_path(custom_history_path.as_ref());

        let history = Box::new(
            FileBackedHistory::with_file(HISTORY_CAPACITY, history_file).unwrap_or_default(),
        );
        let completion_menu = Box::new(
            ColumnarMenu::default()
                .with_name(COMPLETION_MENU)
                .with_marker("")
                .with_text_style(Style::new().bold().fg(Color::Cyan))
                .with_selected_text_style(Style::new().on(Color::White).fg(Color::Black)),
        );

        let edit_mode = Box::new(ForgeEditMode::new(Self::init()));

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
            .use_bracketed_paste(true);
        Self { editor }
    }

    pub fn prompt(&mut self, prompt: &dyn Prompt) -> anyhow::Result<ReadResult> {
        let signal = self.editor.read_line(prompt);
        signal
            .map(Into::into)
            .map_err(|e| anyhow::anyhow!(ReadLineError(e)))
    }

    /// Sets the buffer content to be pre-filled on the next prompt
    pub fn set_buffer(&mut self, content: String) {
        self.editor
            .run_edit_commands(&[EditCommand::InsertString(content)]);
    }
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ReadLineError(std::io::Error);

/// Custom edit mode that wraps Emacs and intercepts paste events.
///
/// When the terminal sends a bracketed-paste (e.g. from a drag-and-drop),
/// this mode checks whether the pasted text is an existing file path and,
/// if so, wraps it in `@[...]` before it reaches the reedline buffer. This
/// gives the user immediate visual feedback in the input field.
struct ForgeEditMode {
    inner: Emacs,
}

impl ForgeEditMode {
    /// Creates a new `ForgeEditMode` wrapping an Emacs mode with the given
    /// keybindings.
    fn new(keybindings: reedline::Keybindings) -> Self {
        Self { inner: Emacs::new(keybindings) }
    }
}

impl EditMode for ForgeEditMode {
    fn parse_event(&mut self, event: ReedlineRawEvent) -> ReedlineEvent {
        // Convert to the underlying crossterm event so we can inspect it
        let raw: Event = event.into();

        if let Event::Paste(ref body) = raw {
            let wrapped = wrap_pasted_text(body);
            return ReedlineEvent::Edit(vec![EditCommand::InsertString(wrapped)]);
        }

        // For every other event, delegate to the inner Emacs mode.
        // We need to reconstruct a ReedlineRawEvent from the crossterm Event.
        // ReedlineRawEvent implements TryFrom<Event>.
        match ReedlineRawEvent::try_from(raw) {
            Ok(raw_event) => self.inner.parse_event(raw_event),
            Err(()) => ReedlineEvent::None,
        }
    }

    fn edit_mode(&self) -> PromptEditMode {
        self.inner.edit_mode()
    }
}

/// Transforms pasted text by wrapping bare file paths in `@[...]` syntax.
///
/// Called when a bracketed-paste event is received. The pasted content is
/// normalised (CRLF to LF) and then each whitespace-delimited token is
/// checked: if it is an absolute path pointing to an existing file it gets
/// wrapped so that forge's attachment parser picks it up.
///
/// Already-wrapped `@[...]` references and non-existent paths are left
/// untouched.
fn wrap_pasted_text(pasted: &str) -> String {
    let normalised = pasted.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalised.trim();

    // If the whole paste is empty, just return normalised form
    if trimmed.is_empty() {
        return normalised;
    }

    // Fast path: the entire paste is a single file path (most common
    // drag-and-drop scenario).
    let clean = strip_surrounding_quotes(trimmed);
    let path = Path::new(clean);
    if path.is_absolute() && path.is_file() {
        return format!("@[{}]", clean);
    }

    // Otherwise scan token by token
    wrap_tokens(&normalised)
}

/// Strips surrounding single or double quotes that some terminals add
/// when dragging files with spaces in their names.
fn strip_surrounding_quotes(s: &str) -> &str {
    if (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('"') && s.ends_with('"'))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Walks through `input` token-by-token and wraps absolute file paths.
fn wrap_tokens(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 32);
    let mut remaining = input;

    while !remaining.is_empty() {
        // Preserve leading whitespace
        let ws_end = remaining
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(remaining.len());
        result.push_str(&remaining[..ws_end]);
        remaining = &remaining[ws_end..];

        if remaining.is_empty() {
            break;
        }

        // Skip already-wrapped @[...] references
        if remaining.starts_with("@[") {
            if let Some(close) = remaining.find(']') {
                result.push_str(&remaining[..=close]);
                remaining = &remaining[close + 1..];
                continue;
            }
        }

        // Extract the next whitespace-delimited token
        let token_end = remaining
            .find(|c: char| c.is_whitespace())
            .unwrap_or(remaining.len());
        let token = &remaining[..token_end];

        let clean = strip_surrounding_quotes(token);
        let path = Path::new(clean);
        if path.is_absolute() && path.is_file() {
            result.push_str(&format!("@[{}]", clean));
        } else {
            result.push_str(token);
        }

        remaining = &remaining[token_end..];
    }

    result
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_wrap_pasted_text_no_paths() {
        let fixture = "hello world";
        let actual = wrap_pasted_text(fixture);
        let expected = "hello world";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_already_wrapped() {
        let fixture = "check @[/usr/bin/env]";
        let actual = wrap_pasted_text(fixture);
        let expected = "check @[/usr/bin/env]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_existing_file() {
        // /usr/bin/env exists on macOS/Linux
        let fixture = "look at /usr/bin/env please";
        let actual = wrap_pasted_text(fixture);
        let expected = "look at @[/usr/bin/env] please";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_nonexistent_path_untouched() {
        let fixture = "look at /nonexistent/path/file.rs please";
        let actual = wrap_pasted_text(fixture);
        let expected = "look at /nonexistent/path/file.rs please";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_bare_path_only() {
        // Just a bare path (typical drag-and-drop result)
        // /usr/bin/env is a real file, so it should be wrapped
        let fixture = "/usr/bin/env";
        let actual = wrap_pasted_text(fixture);
        let expected = "@[/usr/bin/env]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_bare_path_nonexistent() {
        let fixture = "/nonexistent/path/file.rs";
        let actual = wrap_pasted_text(fixture);
        let expected = "/nonexistent/path/file.rs";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_with_text_before() {
        let fixture = "analyze /usr/bin/env";
        let actual = wrap_pasted_text(fixture);
        let expected = "analyze @[/usr/bin/env]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_preserves_whitespace() {
        let fixture = "hello  world";
        let actual = wrap_pasted_text(fixture);
        let expected = "hello  world";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_mixed_existing_and_nonexistent() {
        let fixture = "check /usr/bin/env and /nonexistent/foo.rs";
        let actual = wrap_pasted_text(fixture);
        let expected = "check @[/usr/bin/env] and /nonexistent/foo.rs";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_crlf_normalised() {
        let fixture = "/usr/bin/env\r\n";
        let actual = wrap_pasted_text(fixture);
        let expected = "@[/usr/bin/env]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_wrap_pasted_text_single_quoted_path() {
        let fixture = "'/usr/bin/env'";
        let actual = wrap_pasted_text(fixture);
        let expected = "@[/usr/bin/env]";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_surrounding_quotes_single() {
        let actual = strip_surrounding_quotes("'/some/path'");
        let expected = "/some/path";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_surrounding_quotes_double() {
        let actual = strip_surrounding_quotes("\"/some/path\"");
        let expected = "/some/path";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_surrounding_quotes_none() {
        let actual = strip_surrounding_quotes("/some/path");
        let expected = "/some/path";
        assert_eq!(actual, expected);
    }
}
