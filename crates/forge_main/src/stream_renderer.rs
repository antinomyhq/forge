use std::io;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use colored::Colorize;
use crossterm::cursor;
use crossterm::ExecutableCommand;
use forge_spinner::SpinnerManager;
use streamdown::StreamdownRenderer;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Shared spinner handle for coordination between UI and writer.
pub type SharedSpinner = Arc<Mutex<SpinnerManager>>;

/// Content styling for output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Normal,
    Dimmed,
}

impl Style {
    /// Applies styling to content string.
    fn apply(self, content: String) -> String {
        match self {
            Self::Normal => content,
            Self::Dimmed => content.dimmed().to_string(),
        }
    }
}

/// Streaming markdown writer with automatic spinner management.
///
/// Coordinates between markdown rendering and spinner visibility:
/// - Stops spinner when content is being written
/// - Restarts spinner when the write queue becomes empty
pub struct StreamWriter {
    active: Option<ActiveRenderer>,
    tx: mpsc::UnboundedSender<Command>,
    width: usize,
    handler: JoinHandle<()>,
}

impl StreamWriter {
    /// Creates a new stream writer with the given shared spinner and character delay.
    pub fn new(spinner: SharedSpinner, char_delay_ms: u64) -> Self {
        let width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80);

        let (tx, rx) = mpsc::unbounded_channel();
        let handler = tokio::spawn(writer_task(rx, spinner, char_delay_ms, io::stdout()));

        Self { active: None, tx, width, handler }
    }

    /// Writes markdown content with normal styling.
    pub fn write(&mut self, text: &str) -> Result<()> {
        self.write_styled(text, Style::Normal)
    }

    /// Writes markdown content with dimmed styling (for reasoning blocks).
    pub fn write_dimmed(&mut self, text: &str) -> Result<()> {
        self.write_styled(text, Style::Dimmed)
    }

    /// Flushes all pending content and waits for completion.
    pub async fn flush(&mut self) -> Result<()> {
        // Finish current renderer
        if let Some(active) = self.active.take() {
            let _ = active.renderer.finish();
        }

        // Signal flush and wait for acknowledgment
        let (done_tx, done_rx) = oneshot::channel();
        let _ = self.tx.send(Command::Flush(done_tx));
        let _ = done_rx.await;

        Ok(())
    }

    fn write_styled(&mut self, text: &str, style: Style) -> Result<()> {
        self.ensure_renderer(style);
        if let Some(ref mut active) = self.active {
            active.renderer.push(text)?;
        }
        Ok(())
    }

    fn ensure_renderer(&mut self, new_style: Style) {
        let needs_switch = self.active.as_ref().map_or(false, |a| a.style != new_style);

        if needs_switch {
            if let Some(old) = self.active.take() {
                let _ = old.renderer.finish();
            }
        }

        if self.active.is_none() {
            let writer = ChannelWriter { tx: self.tx.clone(), style: new_style };
            let renderer = StreamdownRenderer::new(writer, self.width);
            self.active = Some(ActiveRenderer { renderer, style: new_style });
        }
    }
}

impl Drop for StreamWriter {
    fn drop(&mut self) {
        self.handler.abort();
    }
}

/// Active renderer with its style.
struct ActiveRenderer {
    renderer: StreamdownRenderer<ChannelWriter>,
    style: Style,
}

/// Commands sent to the writer task.
enum Command {
    Write { content: String, style: Style },
    Flush(oneshot::Sender<()>),
}

/// Bridge between sync `std::io::Write` and async channel.
struct ChannelWriter {
    tx: mpsc::UnboundedSender<Command>,
    style: Style,
}

impl io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Avoid double allocation when UTF-8 is valid
        let content = match std::str::from_utf8(buf) {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8_lossy(buf).into_owned(),
        };
        self.tx
            .send(Command::Write { content, style: self.style })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "writer task closed"))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// RAII guard that suspends the spinner and manages cursor visibility.
///
/// Hides the cursor when created and shows it when dropped.
/// The spinner is stopped when the guard is created and restarted when dropped,
/// based on the restart condition provided at construction.
struct SuspendedSpinner {
    spinner: SharedSpinner,
    should_restart: bool,
}

impl SuspendedSpinner {
    /// Creates a new guard that suspends the spinner and hides the cursor.
    ///
    /// # Arguments
    ///
    /// * `spinner` - Shared spinner to suspend
    /// * `should_restart` - Whether the spinner should restart on drop
    fn new(spinner: SharedSpinner, should_restart: bool) -> Self {
        // Stop the spinner
        if let Ok(mut sp) = spinner.lock() {
            let _ = sp.stop(None);
        }

        // Hide cursor
        let _ = io::stdout().execute(cursor::Hide);

        Self { spinner, should_restart }
    }
}

impl Drop for SuspendedSpinner {
    fn drop(&mut self) {
        // Show cursor
        let _ = io::stdout().execute(cursor::Show);

        // Restart spinner if requested
        if self.should_restart {
            if let Ok(mut sp) = self.spinner.lock() {
                let _ = sp.start(None);
            }
        }
    }
}

/// Async writer task that handles terminal output and spinner coordination.
async fn writer_task<W: io::Write>(
    mut rx: mpsc::UnboundedReceiver<Command>,
    spinner: SharedSpinner,
    _char_delay_ms: u64,
    mut writer: W,
) {
    // let delay = std::time::Duration::from_millis(char_delay_ms);

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Write { content, style } => {
                // Suspend spinner while writing, restart when queue is empty
                let should_restart = rx.is_empty();
                let _guard = SuspendedSpinner::new(spinner.clone(), should_restart);
                let output = style.apply(content);
                let _ = writer.write_all(output.as_bytes());
                let _ = writer.flush();
            }
            Command::Flush(done) => {
                // Suspend spinner while flushing, don't restart after
                let _guard = SuspendedSpinner::new(spinner.clone(), false);
                drain_writes(&mut rx, &mut writer);
                let _ = done.send(());
            }
        }
    }
}

/// Drains all pending write commands from the channel.
fn drain_writes<W: io::Write>(rx: &mut mpsc::UnboundedReceiver<Command>, writer: &mut W) {
    while let Ok(cmd) = rx.try_recv() {
        if let Command::Write { content, style } = cmd {
            let output = style.apply(content);
            let _ = writer.write_all(output.as_bytes());
            let _ = writer.flush();
        }
    }
}
