use std::io;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use colored::Colorize;
use crossterm::ExecutableCommand;
use crossterm::cursor;
use forge_domain::OutputPrinter;
use forge_spinner::SpinnerManager;
use streamdown::StreamdownRenderer;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Shared spinner handle for coordination between UI and writer.
pub type SharedSpinner<P> = Arc<Mutex<SpinnerManager<P>>>;

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
///
/// Generic over the output printer type `P`.
pub struct StreamWriter<P> {
    active: Option<ActiveRenderer>,
    tx: mpsc::UnboundedSender<Command>,
    width: usize,
    handler: JoinHandle<()>,
    _marker: std::marker::PhantomData<P>,
}

impl<P: OutputPrinter + 'static> StreamWriter<P> {
    /// Creates a new stream writer with the given shared spinner and output printer.
    pub fn new(spinner: SharedSpinner<P>, printer: Arc<P>) -> Self {
        let width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80);
        let (tx, rx) = mpsc::unbounded_channel();
        let handler = tokio::spawn(writer_task(rx, spinner, printer));
        Self {
            active: None,
            tx,
            width,
            handler,
            _marker: std::marker::PhantomData,
        }
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

impl<P> Drop for StreamWriter<P> {
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

/// RAII guard that suspends the spinner.
///
/// The spinner is stopped when the guard is created and restarted when dropped,
/// based on the restart condition provided at construction.
struct SuspendedSpinner<P: OutputPrinter> {
    spinner: SharedSpinner<P>,
    should_restart: bool,
}

impl<P: OutputPrinter> SuspendedSpinner<P> {
    /// Creates a new guard that suspends the spinner.
    fn new(spinner: SharedSpinner<P>, should_restart: bool) -> Self {
        if let Ok(mut sp) = spinner.lock() {
            let _ = sp.stop(None);
        }
        Self { spinner, should_restart }
    }
}

impl<P: OutputPrinter> Drop for SuspendedSpinner<P> {
    fn drop(&mut self) {
        if self.should_restart {
            if let Ok(mut sp) = self.spinner.lock() {
                let _ = sp.start(None);
            }
        }
    }
}

/// Wrapper that ensures cursor is shown on drop and uses OutputPrinter.
struct PrinterGuard<P: OutputPrinter> {
    printer: Arc<P>,
    cursor_hidden: bool,
}

impl<P: OutputPrinter> PrinterGuard<P> {
    fn new(printer: Arc<P>) -> Self {
        Self { printer, cursor_hidden: false }
    }

    /// Hides the cursor.
    fn hide_cursor(&mut self) {
        let mut buf = Vec::new();
        let _ = buf.execute(cursor::Hide);
        let _ = self.printer.write(&buf);
        let _ = self.printer.flush();
        self.cursor_hidden = true;
    }

    /// Shows the cursor.
    fn show_cursor(&mut self) {
        let mut buf = Vec::new();
        let _ = buf.execute(cursor::Show);
        let _ = self.printer.write(&buf);
        let _ = self.printer.flush();
        self.cursor_hidden = false;
    }

    /// Writes content to primary output.
    fn write(&self, content: &str) {
        let _ = self.printer.write(content.as_bytes());
        let _ = self.printer.flush();
    }
}

impl<P: OutputPrinter> Drop for PrinterGuard<P> {
    fn drop(&mut self) {
        if self.cursor_hidden {
            self.show_cursor();
        }
    }
}

/// Async writer task that handles terminal output and spinner coordination.
async fn writer_task<P: OutputPrinter>(
    mut rx: mpsc::UnboundedReceiver<Command>,
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
) {
    let mut guard = PrinterGuard::new(printer);

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Write { content, style } => {
                // Suspend spinner while writing, restart when queue is empty
                let _guard = SuspendedSpinner::new(spinner.clone(), rx.is_empty());
                guard.hide_cursor();
                let output = style.apply(content);
                guard.write(&output);
                guard.show_cursor();
            }
            Command::Flush(done) => {
                // Suspend spinner while flushing, don't restart after
                let _guard = SuspendedSpinner::new(spinner.clone(), false);
                guard.hide_cursor();
                drain_writes(&mut rx, &mut guard);
                guard.show_cursor();
                let _ = done.send(());
            }
        }
    }
}

/// Drains all pending write commands from the channel.
fn drain_writes<P: OutputPrinter>(
    rx: &mut mpsc::UnboundedReceiver<Command>,
    guard: &mut PrinterGuard<P>,
) {
    while let Ok(cmd) = rx.try_recv() {
        if let Command::Write { content, style } = cmd {
            let output = style.apply(content);
            guard.write(&output);
        }
    }
}
