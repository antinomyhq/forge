//! Demo of streaming LLM output with markdown rendering.

use std::io::{self, Write};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use forge_markdown_stream::StreamdownRenderer;

/// A spinner that only shows during idle periods (no content for a while).
struct Spinner {
    inner: Option<ProgressBar>,
    message: String,
}

impl Spinner {
    fn new(message: impl Into<String>) -> Self {
        Self {
            inner: None,
            message: message.into(),
        }
    }

    /// Show the spinner (called after timeout with no content).
    fn show(&mut self) {
        if self.inner.is_some() {
            return;
        }
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.green} {msg} {elapsed:.white}")
                .unwrap(),
        );
        pb.set_message(self.message.clone());
        pb.enable_steady_tick(Duration::from_millis(60));
        self.inner = Some(pb);
    }

    /// Hide the spinner (called when content arrives).
    fn hide(&mut self) {
        if let Some(pb) = self.inner.take() {
            pb.finish_and_clear();
        }
    }

    fn is_visible(&self) -> bool {
        self.inner.is_some()
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.hide();
    }
}

/// A writer that outputs content character-by-character with small delays in a background thread.
/// Shows a spinner only during idle periods.
struct CharWriter {
    sender: Sender<Option<String>>,
    handle: Option<JoinHandle<()>>,
}

impl CharWriter {
    fn new(delay_ms: u64) -> Self {
        let (sender, receiver) = mpsc::channel::<Option<String>>();
        let delay = Duration::from_millis(delay_ms);
        let idle_timeout = Duration::from_millis(50);

        let handle = thread::spawn(move || {
            let mut stdout = io::stdout();
            let mut spinner = Spinner::new("Waiting for response...");

            loop {
                // Try to receive with timeout
                match receiver.recv_timeout(idle_timeout) {
                    Ok(Some(content)) => {
                        // Content arrived - hide spinner if showing
                        spinner.hide();

                        // Print content character by character
                        for ch in content.chars() {
                            let mut buf = [0u8; 4];
                            let encoded = ch.encode_utf8(&mut buf);
                            let _ = stdout.write_all(encoded.as_bytes());
                            let _ = stdout.flush();
                            if !delay.is_zero() {
                                thread::sleep(delay);
                            }
                        }
                    }
                    Ok(None) => {
                        // Finish signal
                        spinner.hide();
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // No content for a while - show spinner
                        if !spinner.is_visible() {
                            spinner.show();
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        spinner.hide();
                        break;
                    }
                }
            }
        });

        Self {
            sender,
            handle: Some(handle),
        }
    }
}

impl Write for CharWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf).into_owned();
        self.sender
            .send(Some(s))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "writer thread gone"))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for CharWriter {
    fn drop(&mut self) {
        let _ = self.sender.send(None);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn main() -> io::Result<()> {
    let content = include_str!("data.md");
    let tokens: Vec<&str> = content.split("<separator>").collect();
    let writer = CharWriter::new(1);
    let mut renderer = StreamdownRenderer::new(writer, 80);

    for token in &tokens {
        renderer.push(token)?;
    }

    renderer.finish()?;

    Ok(())
}
