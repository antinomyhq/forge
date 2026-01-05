use std::io;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use colored::Colorize;
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

/// Executes an async function while the spinner is suspended.
///
/// Stops the spinner before executing the async function, then conditionally restarts it based on the predicate.
async fn with_suspended_spinner<F, Fut, P>(
    spinner: &SharedSpinner,
    f: F,
    should_restart: P,
) -> Fut::Output
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future,
    P: FnOnce() -> bool,
{
    if let Ok(mut sp) = spinner.lock() {
        let _ = sp.stop(None);
    }

    let result = f().await;

    if should_restart() {
        if let Ok(mut sp) = spinner.lock() {
            let _ = sp.start(None);
        }
    }

    result
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
                with_suspended_spinner(
                    &spinner,
                    || async {
                        let output = style.apply(content);
                        let _ = writer.write_all(output.as_bytes());
                        let _ = writer.flush();
                    },
                    || rx.is_empty(), // Restart spinner when queue is empty
                )
                .await;
            }
            Command::Flush(done) => {
                // Drain pending writes before acknowledging
                with_suspended_spinner(
                    &spinner,
                    || async {
                        drain_writes(&mut rx, &mut writer);
                        let _ = done.send(());
                    },
                    || false,
                )
                .await;
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
