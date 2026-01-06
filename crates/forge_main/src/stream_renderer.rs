use std::io;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use colored::Colorize;
use forge_domain::OutputPrinter;
use forge_spinner::SpinnerManager;
use streamdown::StreamdownRenderer;

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
/// - Restarts spinner when idle
///
/// Generic over the output printer type `P`.
pub struct StreamWriter<P: OutputPrinter> {
    active: Option<ActiveRenderer<P>>,
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
}

fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

impl<P: OutputPrinter + 'static> StreamWriter<P> {
    /// Creates a new stream writer with the given shared spinner and output printer.
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
        let needs_switch = self.active.as_ref().map_or(false, |a| a.style != new_style);

        if needs_switch {
            if let Some(old) = self.active.take() {
                let _ = old.finish()?;
            }
        }

        if self.active.is_none() {
            let writer = DirectWriter {
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
struct ActiveRenderer<P: OutputPrinter> {
    renderer: StreamdownRenderer<DirectWriter<P>>,
    style: Style,
}

impl<P: OutputPrinter> ActiveRenderer<P> {
    pub fn push(&mut self, text: &str) -> Result<()> {
        self.renderer.push(text)?;
        Ok(())
    }

    pub fn finish(self) -> Result<()> {
        self.renderer.finish()?;
        Ok(())
    }
}

/// Direct writer that writes to printer and manages spinner.
struct DirectWriter<P: OutputPrinter> {
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
    style: Style,
}

impl<P: OutputPrinter> DirectWriter<P> {
    fn pause_spinner(&self) {
        if let Ok(mut sp) = self.spinner.lock() {
            let _ = sp.stop(None);
        }
    }

    fn resume_spinner(&self) {
        if let Ok(mut sp) = self.spinner.lock() {
            let _ = sp.start(None);
        }
    }
}

impl<P: OutputPrinter> Drop for DirectWriter<P> {
    fn drop(&mut self) {
        let _ = self.printer.flush();
        let _ = self.printer.flush_err();
    }
}

impl<P: OutputPrinter> io::Write for DirectWriter<P> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pause_spinner();

        let content = match std::str::from_utf8(buf) {
            Ok(s) => s.to_string(),
            Err(_) => String::from_utf8_lossy(buf).into_owned(),
        };
        let styled = self.style.apply(content);
        let _ = self.printer.write(styled.as_bytes());
        let _ = self.printer.flush();

        // Track if we ended on a newline - only safe to show spinner at line start
        if buf.last() == Some(&b'\n') {
            self.resume_spinner();
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = self.printer.flush();
        Ok(())
    }
}
