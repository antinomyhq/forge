use std::io;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use colored::Colorize;
use forge_display::MarkdownFormat;
use forge_domain::OutputPrinter;
use forge_markdown_stream::StreamdownRenderer;
use forge_spinner::SpinnerManager;

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

/// Unified content writer that handles both streaming and direct output modes.
/// - `Streaming`: Renders markdown incrementally as chunks arrive (uses
///   streamdown)
/// - `Direct`: Renders markdown immediately in full (uses MarkdownFormat)
pub enum ContentWriter<P: OutputPrinter> {
    Streaming(StreamingWriter<P>),
    Direct(DirectContentWriter<P>),
}

impl<P: OutputPrinter + 'static> ContentWriter<P> {
    /// Creates a new streaming content writer.
    pub fn streaming(spinner: SharedSpinner<P>, printer: Arc<P>) -> Self {
        Self::Streaming(StreamingWriter::new(spinner, printer))
    }

    /// Creates a new direct content writer.
    pub fn direct(spinner: SharedSpinner<P>, printer: Arc<P>, markdown: MarkdownFormat) -> Self {
        Self::Direct(DirectContentWriter::new(spinner, printer, markdown))
    }

    /// Writes markdown content with normal styling.
    pub fn write(&mut self, text: &str) -> Result<()> {
        match self {
            Self::Streaming(w) => w.write(text),
            Self::Direct(w) => w.write(text),
        }
    }

    /// Writes markdown content with dimmed styling (for reasoning blocks).
    pub fn write_dimmed(&mut self, text: &str) -> Result<()> {
        match self {
            Self::Streaming(w) => w.write_dimmed(text),
            Self::Direct(w) => w.write_dimmed(text),
        }
    }

    /// Finishes any pending rendering.
    pub fn finish(&mut self) -> Result<()> {
        match self {
            Self::Streaming(w) => w.finish(),
            Self::Direct(w) => w.finish(),
        }
    }
}

/// Direct content writer that renders markdown immediately using
/// MarkdownFormat.
pub struct DirectContentWriter<P: OutputPrinter> {
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
    markdown: MarkdownFormat,
}

impl<P: OutputPrinter> DirectContentWriter<P> {
    /// Creates a new direct content writer.
    pub fn new(spinner: SharedSpinner<P>, printer: Arc<P>, markdown: MarkdownFormat) -> Self {
        Self { spinner, printer, markdown }
    }

    /// Writes markdown content with normal styling.
    pub fn write(&mut self, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        self.pause_spinner();
        let rendered = self.markdown.render(text);
        let _ = self.printer.write(rendered.as_bytes());
        let _ = self.printer.write(b"\n");
        let _ = self.printer.flush();
        self.resume_spinner();
        Ok(())
    }

    /// Writes markdown content with dimmed styling.
    pub fn write_dimmed(&mut self, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        self.pause_spinner();
        let rendered = self.markdown.render(text);
        let styled = rendered.dimmed().to_string();
        let _ = self.printer.write(styled.as_bytes());
        let _ = self.printer.write(b"\n");
        let _ = self.printer.flush();
        self.resume_spinner();
        Ok(())
    }

    /// No-op for direct writer - content is already rendered.
    pub fn finish(&mut self) -> Result<()> {
        Ok(())
    }

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
pub struct StreamingWriter<P: OutputPrinter> {
    active: Option<ActiveRenderer<P>>,
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
}

impl<P: OutputPrinter + 'static> StreamingWriter<P> {
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
struct ActiveRenderer<P: OutputPrinter> {
    renderer: StreamdownRenderer<StreamDirectWriter<P>>,
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

/// Writer for streamdown that outputs to printer and manages spinner.
struct StreamDirectWriter<P: OutputPrinter> {
    spinner: SharedSpinner<P>,
    printer: Arc<P>,
    style: Style,
}

impl<P: OutputPrinter> StreamDirectWriter<P> {
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

impl<P: OutputPrinter> Drop for StreamDirectWriter<P> {
    fn drop(&mut self) {
        let _ = self.printer.flush();
        let _ = self.printer.flush_err();
    }
}

impl<P: OutputPrinter> io::Write for StreamDirectWriter<P> {
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
