use std::io;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use colored::Colorize;
use forge_domain::ConsoleWriter;
use forge_markdown_stream::StreamdownRenderer;
use forge_spinner::SpinnerManager;

/// Shared spinner wrapper that encapsulates locking for thread-safe spinner
/// operations.
///
/// Provides the same API as `SpinnerManager` but handles mutex locking
/// internally, releasing the lock immediately after each operation completes.
pub struct SharedSpinner<P: ConsoleWriter>(Arc<Mutex<SpinnerManager<P>>>);

impl<P: ConsoleWriter> Clone for SharedSpinner<P> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<P: ConsoleWriter> SharedSpinner<P> {
    /// Creates a new shared spinner from a SpinnerManager.
    pub fn new(spinner: SpinnerManager<P>) -> Self {
        Self(Arc::new(Mutex::new(spinner)))
    }

    /// Start the spinner with a message.
    pub fn start(&self, message: Option<&str>) -> Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .start(message)
    }

    /// Stop the active spinner if any.
    pub fn stop(&self, message: Option<String>) -> Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stop(message)
    }

    /// Resets the stopwatch to zero.
    pub fn reset(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).reset()
    }

    /// Writes a line to stdout, suspending the spinner if active.
    pub fn write_ln(&self, message: impl ToString) -> Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .write_ln(message)
    }

    /// Returns whether the spinner is currently active (running).
    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).is_active()
    }

    /// Writes a line to stderr, suspending the spinner if active.
    pub fn ewrite_ln(&self, message: impl ToString) -> Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ewrite_ln(message)
    }
}

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

fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

/// Streaming markdown writer with automatic spinner management.
///
/// Coordinates between markdown rendering and spinner visibility:
/// - Stops spinner when content is being written
/// - Restarts spinner when idle
pub struct StreamingWriter<P: ConsoleWriter> {
    active: Option<ActiveRenderer<P>>,
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
}

impl<P: ConsoleWriter + 'static> StreamingWriter<P> {
    /// Creates a new stream writer with the given shared spinner and output
    /// printer.
    pub fn new(spinner: SharedSpinner<P>, printer: Arc<P>) -> Self {
        Self { active: None, spinner, printer }
    }

    /// Writes markdown content with normal styling.
    pub fn write(&mut self, text: &str) -> Result<()> {
        self.write_styled(text, Style::Normal)
    }

    /// Writes markdown content with dimmed styling (for reasoning blocks).
    pub fn write_dimmed(&mut self, text: &str) -> Result<()> {
        self.write_styled(text, Style::Dimmed)
    }

    /// Finishes any active renderer.
    pub fn finish(&mut self) -> Result<()> {
        if let Some(active) = self.active.take() {
            active.finish()?;
        }
        Ok(())
    }

    fn write_styled(&mut self, text: &str, style: Style) -> Result<()> {
        self.ensure_renderer(style)?;
        if let Some(ref mut active) = self.active {
            active.push(text)?;
        }
        Ok(())
    }

    fn ensure_renderer(&mut self, new_style: Style) -> Result<()> {
        let needs_switch = self.active.as_ref().is_some_and(|a| a.style != new_style);

        if needs_switch && let Some(old) = self.active.take() {
            old.finish()?;
        }

        if self.active.is_none() {
            let writer = StreamDirectWriter {
                spinner: self.spinner.clone(),
                printer: self.printer.clone(),
                style: new_style,
            };
            let renderer = StreamdownRenderer::new(writer, term_width());
            self.active = Some(ActiveRenderer { renderer, style: new_style });
        }
        Ok(())
    }
}

/// Active renderer with its style.
struct ActiveRenderer<P: ConsoleWriter> {
    renderer: StreamdownRenderer<StreamDirectWriter<P>>,
    style: Style,
}

impl<P: ConsoleWriter> ActiveRenderer<P> {
    pub fn push(&mut self, text: &str) -> Result<()> {
        self.renderer.push(text)?;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        self.renderer.finish()?;
        Ok(())
    }
}

/// Writer for streamdown that outputs to printer and manages spinner.
struct StreamDirectWriter<P: ConsoleWriter> {
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
    style: Style,
}

impl<P: ConsoleWriter> StreamDirectWriter<P> {
    fn pause_spinner(&self) {
        let _ = self.spinner.stop(None);
    }
}

impl<P: ConsoleWriter> Drop for StreamDirectWriter<P> {
    fn drop(&mut self) {
        // Stop the spinner to prevent indicatif's finish_and_clear() from
        // erasing content lines. Without this, the spinner remains active
        // after the writer is dropped (from resume_spinner in write()),
        // and its background thread can overwrite terminal content.
        let _ = self.spinner.stop(None);
        let _ = self.printer.flush();
        let _ = self.printer.flush_err();
    }
}

impl<P: ConsoleWriter> io::Write for StreamDirectWriter<P> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pause_spinner();

        let content = match std::str::from_utf8(buf) {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8_lossy(buf).into_owned(),
        };
        let styled = self.style.apply(content);
        self.printer.write(styled.as_bytes())?;
        self.printer.flush()?;

        // NOTE: We intentionally do NOT restart the spinner here.
        // The spinner lifecycle is managed by the UI layer (ToolCallEnd
        // handler restarts it). Restarting on every newline caused a race
        // condition where indicatif's finish_and_clear() would erase
        // content lines when the spinner was later stopped.

        // Return `buf.len()`, not `styled.as_bytes().len()`. The `io::Write` contract
        // requires returning how many bytes were consumed from the input buffer, not
        // how many bytes were written to the output. Styling adds ANSI escape codes
        // which makes the output larger than the input.
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.printer.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use forge_domain::ConsoleWriter;
    use forge_spinner::SpinnerManager;
    use pretty_assertions::assert_eq;

    use super::{SharedSpinner, StreamingWriter};

    /// Mock writer that captures all output into a buffer.
    #[derive(Clone)]
    struct MockWriter {
        stdout: Arc<Mutex<Vec<u8>>>,
        stderr: Arc<Mutex<Vec<u8>>>,
    }

    impl MockWriter {
        fn new() -> Self {
            Self {
                stdout: Arc::new(Mutex::new(Vec::new())),
                stderr: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn stdout_content(&self) -> String {
            let buf = self.stdout.lock().unwrap();
            String::from_utf8_lossy(&buf).to_string()
        }
    }

    impl ConsoleWriter for MockWriter {
        fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
            self.stdout.lock().unwrap().write(buf)
        }

        fn write_err(&self, buf: &[u8]) -> std::io::Result<usize> {
            self.stderr.lock().unwrap().write(buf)
        }

        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }

        fn flush_err(&self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn fixture() -> (
        StreamingWriter<MockWriter>,
        SharedSpinner<MockWriter>,
        MockWriter,
    ) {
        let mock = MockWriter::new();
        let printer = Arc::new(mock.clone());
        let spinner = SharedSpinner::new(SpinnerManager::new(printer.clone()));
        let writer = StreamingWriter::new(spinner.clone(), printer);
        (writer, spinner, mock)
    }

    /// After writing content ending with newlines and calling finish(),
    /// the spinner must be inactive. A lingering active spinner causes
    /// indicatif's finish_and_clear() to erase content lines when it is
    /// eventually stopped elsewhere.
    #[test]
    fn test_spinner_inactive_after_finish() {
        let (mut writer, spinner, _mock) = fixture();

        // Start spinner (simulating the state when LLM starts responding)
        spinner.start(None).unwrap();

        // Write several lines of content — each newline triggers resume_spinner
        writer.write("Line one\n").unwrap();
        writer.write("Line two\n").unwrap();
        writer.write("Line three\n").unwrap();

        // Finish the writer (as happens on TaskComplete)
        writer.finish().unwrap();

        let actual = spinner.is_active();
        let expected = false;
        assert_eq!(actual, expected, "spinner must be inactive after finish()");
    }

    /// Same invariant but via implicit drop instead of explicit finish().
    #[test]
    fn test_spinner_inactive_after_drop() {
        let (mut writer, spinner, _mock) = fixture();

        spinner.start(None).unwrap();

        writer.write("Line one\n").unwrap();
        writer.write("Line two\n").unwrap();

        // Drop the writer without calling finish()
        drop(writer);

        let actual = spinner.is_active();
        let expected = false;
        assert_eq!(actual, expected, "spinner must be inactive after drop");
    }

    /// All content written through StreamingWriter must be preserved
    /// in the output buffer after finish().
    #[test]
    fn test_content_preserved_after_finish() {
        let (mut writer, _spinner, mock) = fixture();

        writer.write("Hello world\n").unwrap();
        writer.write("Second line\n").unwrap();
        writer.finish().unwrap();

        let actual = mock.stdout_content();
        assert!(
            actual.contains("Hello world"),
            "output must contain 'Hello world', got: {actual}"
        );
        assert!(
            actual.contains("Second line"),
            "output must contain 'Second line', got: {actual}"
        );
    }
}
