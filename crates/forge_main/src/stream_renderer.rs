//! Streaming markdown renderer wrapper for LLM output.
//!
//! Provides a unified interface for rendering content and reasoning streams.
//! Only one stream can be active at a time.

use std::io::{self, Write};

use colored::Colorize;
use forge_spinner::SpinnerManager;
use streamdown::StreamdownRenderer;

/// A writer that suspends the spinner before writing.
struct SpinnerWriter {
    spinner: SpinnerManager,
    dimmed: bool,
}

impl SpinnerWriter {
    fn new(spinner: SpinnerManager) -> Self {
        Self { spinner, dimmed: false }
    }

    fn dimmed(spinner: SpinnerManager) -> Self {
        Self { spinner, dimmed: true }
    }
}

impl Write for SpinnerWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        let output = if self.dimmed { s.dimmed().to_string() } else { s.to_string() };
        self.spinner
            .write(output)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The active renderer type.
enum Renderer {
    Content(StreamdownRenderer<SpinnerWriter>),
    Reasoning(StreamdownRenderer<SpinnerWriter>),
}

impl Renderer {
    fn content(spinner: SpinnerManager, width: usize) -> Self {
        Self::Content(StreamdownRenderer::new(SpinnerWriter::new(spinner), width))
    }

    fn reasoning(spinner: SpinnerManager, width: usize) -> Self {
        Self::Reasoning(StreamdownRenderer::new(SpinnerWriter::dimmed(spinner), width))
    }

    fn is_content(&self) -> bool {
        matches!(self, Self::Content(_))
    }

    fn is_reasoning(&self) -> bool {
        matches!(self, Self::Reasoning(_))
    }

    fn push(&mut self, text: &str) -> io::Result<()> {
        match self {
            Self::Content(r) | Self::Reasoning(r) => r.push(text),
        }
    }

    fn finish(self) -> io::Result<()> {
        match self {
            Self::Content(r) | Self::Reasoning(r) => r.finish(),
        }
    }
}

/// Wrapper around StreamdownRenderer that manages content and reasoning streams.
///
/// Only one stream can be active at a time. When switching between stream types,
/// the previous stream is automatically finished.
pub struct ChatStreamRenderer {
    renderer: Option<Renderer>,
    spinner: SpinnerManager,
    width: usize,
}

impl ChatStreamRenderer {
    /// Creates a new renderer with the given spinner and terminal width.
    pub fn new(spinner: SpinnerManager, width: usize) -> Self {
        Self { renderer: None, spinner, width }
    }

    /// Creates a new renderer using the current terminal width.
    pub fn from_terminal(spinner: SpinnerManager) -> Self {
        let width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(80);
        Self::new(spinner, width)
    }

    /// Flush and finish the current renderer if active.
    ///
    /// Call this before outputting non-streamed content (titles, tool calls, etc.)
    pub fn flush(&mut self) -> io::Result<()> {
        if let Some(renderer) = self.renderer.take() {
            renderer.finish()?;
        }
        Ok(())
    }

    /// Push content to the content stream.
    ///
    /// If reasoning stream is active, it will be finished first.
    pub fn push_content(&mut self, text: &str) -> io::Result<()> {
        // If reasoning is active, finish it first
        if self.renderer.as_ref().is_some_and(|r| r.is_reasoning()) {
            self.flush()?;
        }

        // Initialize content renderer if needed
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::content(self.spinner.clone(), self.width));
        }

        if let Some(ref mut renderer) = self.renderer {
            renderer.push(text)?;
        }

        Ok(())
    }

    /// Push content to the reasoning stream.
    ///
    /// If content stream is active, it will be finished first.
    pub fn push_reasoning(&mut self, text: &str) -> io::Result<()> {
        // If content is active, finish it first
        if self.renderer.as_ref().is_some_and(|r| r.is_content()) {
            self.flush()?;
        }

        // Initialize reasoning renderer if needed
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::reasoning(self.spinner.clone(), self.width));
        }

        if let Some(ref mut renderer) = self.renderer {
            renderer.push(text)?;
        }

        Ok(())
    }

    /// Finish all streams and consume the renderer.
    pub fn finish(mut self) -> io::Result<()> {
        self.flush()
    }
}
