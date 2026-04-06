use std::path::PathBuf;
use std::sync::Arc;

use forge_api::Environment;
use nu_ansi_term::{Color, Style};
use reedline::{
    ColumnarMenu, DefaultHinter, EditCommand, Emacs, FileBackedHistory, KeyCode, KeyModifiers,
    MenuBuilder, Prompt, Reedline, ReedlineEvent, ReedlineMenu, Signal, default_emacs_keybindings,
};

use super::completer::InputCompleter;
use crate::model::ForgeCommandManager;

// TODO: Store the last `HISTORY_CAPACITY` commands in the history file
const HISTORY_CAPACITY: usize = 1024 * 1024;
const COMPLETION_MENU: &str = "completion_menu";
/// Zero-width space used as an invisible submit marker for Shift+Tab agent cycling.
/// This is an implementation detail — higher layers see `ReadResult::CycleAgent`.
const CYCLE_AGENT_MARKER: &str = "\u{200B}";

pub struct ForgeEditor {
    editor: Reedline,
}

pub enum ReadResult {
    Success(String),
    CycleAgent,
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

        // on Shift+Tab press cycles to the next agent.
        // Uses a proper Submit (not ExecuteHostCommand) so reedline correctly
        // commits the prompt line. The zero-width space is invisible to the user
        // but survives trim(), letting us distinguish this from an empty Enter.
        let cycle = ReedlineEvent::Multiple(vec![
            ReedlineEvent::Edit(vec![
                EditCommand::Clear,
                EditCommand::InsertString(CYCLE_AGENT_MARKER.to_string()),
            ]),
            ReedlineEvent::Submit,
        ]);
        keybindings.add_binding(KeyModifiers::SHIFT, KeyCode::BackTab, cycle.clone());
        keybindings.add_binding(KeyModifiers::NONE, KeyCode::BackTab, cycle);

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
            .use_bracketed_paste(true);
        Self { editor }
    }

    pub fn prompt(&mut self, prompt: &dyn Prompt) -> anyhow::Result<ReadResult> {
        // Discard any bytes buffered in stdin during agent execution
        // (e.g. Shift+Tab presses) so they don't trigger actions.
        drain_stdin();
        self.editor
            .read_line(prompt)
            .map(Into::into)
            .map_err(|e| anyhow::anyhow!(ReadLineError(e)))
    }

    /// Sets the buffer content to be pre-filled on the next prompt
    pub fn set_buffer(&mut self, content: String) {
        self.editor
            .run_edit_commands(&[EditCommand::InsertString(content)]);
    }
}

/// Reads and discards any bytes pending in stdin by briefly setting it to
/// non-blocking mode. This prevents buffered keypresses from agent execution
/// (like Shift+Tab) from being interpreted by reedline.
#[cfg(unix)]
fn drain_stdin() {
    use std::io::Read;
    use std::os::unix::io::AsRawFd;

    unsafe extern "C" {
        fn fcntl(fd: std::ffi::c_int, cmd: std::ffi::c_int, ...) -> std::ffi::c_int;
    }

    const F_GETFL: std::ffi::c_int = 3;
    const F_SETFL: std::ffi::c_int = 4;
    #[cfg(target_os = "linux")]
    const O_NONBLOCK: std::ffi::c_int = 0o4000;
    #[cfg(not(target_os = "linux"))]
    const O_NONBLOCK: std::ffi::c_int = 0x0004;

    let fd = std::io::stdin().as_raw_fd();
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 {
        return;
    }

    if unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        return;
    }

    let mut buf = [0u8; 1024];
    let mut stdin = std::io::stdin().lock();
    while stdin.read(&mut buf).unwrap_or(0) > 0 {}
    drop(stdin);

    unsafe { fcntl(fd, F_SETFL, flags) };
}

#[cfg(not(unix))]
fn drain_stdin() {}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ReadLineError(std::io::Error);

impl From<Signal> for ReadResult {
    fn from(signal: Signal) -> Self {
        match signal {
            Signal::Success(buffer) => {
                let trimmed = buffer.trim();
                if trimmed == CYCLE_AGENT_MARKER {
                    ReadResult::CycleAgent
                } else if trimmed.is_empty() {
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
