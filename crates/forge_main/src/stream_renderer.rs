use std::io::{self, Write as _};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use forge_spinner::SpinnerManager;
use streamdown::StreamdownRenderer;
use tokio::sync::{mpsc, oneshot};

// ============================================================================
// Public API
// ============================================================================

/// Shared spinner handle for coordination between UI and writer.
pub type SharedSpinner = Arc<Mutex<SpinnerManager>>;

/// Content styling for output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Normal,
    Dimmed,
}

/// Streaming markdown writer with automatic spinner management.
///
/// Coordinates between markdown rendering and spinner visibility:
/// - Stops spinner when content is being written
/// - Restarts spinner when the write queue becomes empty
pub struct StreamWriter {
    active: Option<ActiveRenderer>,
    tx: mpsc::UnboundedSender<Command>,
    task: tokio::task::JoinHandle<()>,
    spinner: SharedSpinner,
    width: usize,
    char_delay: Duration,
}

impl StreamWriter {
    /// Creates a new stream writer with the given shared spinner.
    pub fn new(spinner: SharedSpinner) -> Self {
        let width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80);
        let char_delay = Duration::from_millis(1);

        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(writer_task(rx, Arc::clone(&spinner), char_delay));

        Self {
            active: None,
            tx,
            task,
            spinner,
            width,
            char_delay,
        }
    }

    /// Writes markdown content with normal styling.
    pub fn write(&mut self, text: &str) -> Result<()> {
        self.ensure_renderer(Style::Normal)?;
        if let Some(ref mut active) = self.active {
            active.renderer.push(text)?;
        }
        Ok(())
    }

    /// Writes markdown content with dimmed styling (for reasoning blocks).
    pub fn write_dimmed(&mut self, text: &str) -> Result<()> {
        self.ensure_renderer(Style::Dimmed)?;
        if let Some(ref mut active) = self.active {
            active.renderer.push(text)?;
        }
        Ok(())
    }

    /// Flushes all pending content and waits for completion.
    pub async fn flush(&mut self) -> Result<()> {
        // Finish renderer and add newline
        if let Some(old) = self.active.take() {
            let _ = old.renderer.finish();
        }

        // Shutdown current task and wait
        let (done_tx, done_rx) = oneshot::channel();
        let _ = self.tx.send(Command::Shutdown(done_tx));
        let _ = done_rx.await;

        // Spawn new task for future writes
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(writer_task(rx, Arc::clone(&self.spinner), self.char_delay));
        
        // Replace old task with new one
        let old_task = std::mem::replace(&mut self.task, task);
        let _ = old_task.await;
        self.tx = tx;

        Ok(())
    }

    fn ensure_renderer(&mut self, new_style: Style) -> Result<()> {
        let needs_switch = self.active.as_ref().map_or(false, |a| a.style != new_style);

        if needs_switch {
            // Finish current renderer and add newline
            if let Some(old) = self.active.take() {
                let _ = old.renderer.finish();
            }
        }

        if self.active.is_none() {
            let writer = ChannelWriter {
                tx: self.tx.clone(),
                style: new_style,
            };
            let renderer = StreamdownRenderer::new(writer, self.width);
            self.active = Some(ActiveRenderer {
                renderer,
                style: new_style,
            });
        }

        Ok(())
    }
}

// ============================================================================
// Internal Implementation
// ============================================================================

/// Active renderer with its style.
struct ActiveRenderer {
    renderer: StreamdownRenderer<ChannelWriter>,
    style: Style,
}

/// Commands sent to the writer task.
enum Command {
    Write { content: String, style: Style },
    Shutdown(oneshot::Sender<()>),
}

/// Bridge between sync `std::io::Write` and async channel.
struct ChannelWriter {
    tx: mpsc::UnboundedSender<Command>,
    style: Style,
}

impl io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let content = String::from_utf8_lossy(buf).to_string();
        self.tx
            .send(Command::Write {
                content,
                style: self.style,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "writer task closed"))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Async writer task that handles terminal output and spinner coordination.
async fn writer_task(
    mut rx: mpsc::UnboundedReceiver<Command>,
    spinner: SharedSpinner,
    char_delay: Duration,
) {
    let mut stdout = io::stdout();
    let mut is_writing = false;

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Write { content, style } => {
                // Stop spinner on first write
                if !is_writing {
                    if let Ok(mut sp) = spinner.lock() {
                        let _ = sp.stop(None);
                    }
                    is_writing = true;
                }

                // Apply styling
                let output = match style {
                    Style::Normal => content,
                    Style::Dimmed => content.dimmed().to_string(),
                };

                // Write with optional char-by-char delay
                if char_delay.is_zero() {
                    let _ = stdout.write_all(output.as_bytes());
                    let _ = stdout.flush();
                } else {
                    for ch in output.chars() {
                        let mut buf = [0u8; 4];
                        let _ = stdout.write_all(ch.encode_utf8(&mut buf).as_bytes());
                        let _ = stdout.flush();
                        tokio::time::sleep(char_delay).await;
                    }
                }

                // Restart spinner when queue is empty
                if rx.is_empty() {
                    if let Ok(mut sp) = spinner.lock() {
                        let _ = sp.start(None);
                    }
                    is_writing = false;
                }
            }
            Command::Shutdown(done) => {
                // Drain all pending writes before acknowledging shutdown
                while let Ok(cmd) = rx.try_recv() {
                    if let Command::Write { content, style } = cmd {
                        let output = match style {
                            Style::Normal => content,
                            Style::Dimmed => content.dimmed().to_string(),
                        };
                        // Flush immediately without delay during shutdown
                        let _ = stdout.write_all(output.as_bytes());
                        let _ = stdout.flush();
                    }
                }
                let _ = done.send(());
                break;
            }
        }
    }
}
