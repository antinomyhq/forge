//! Unified spinner and markdown streaming for terminal UI.
//!
//! This module provides a single `SpinnerManager` that handles both:
//! - Standalone spinner display (for loading states) - uses indicatif
//! - Progressive markdown streaming with spinner below content - uses crossterm

use std::io::{self, Write};

use anyhow::Result;
use colored::Colorize;
use crossterm::{
    cursor::{position, MoveToColumn, MoveUp},
    execute,
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use indicatif::{ProgressBar, ProgressStyle};
use mdstream::{
    ProgressiveRenderer,
    writer::{Position, TermWriter, Writer},
};
use rand::Rng;
use tokio::task::JoinHandle;

mod progress_bar;
mod stopwatch;

pub use progress_bar::*;
pub use stopwatch::Stopwatch;

// Re-export mdstream types for advanced usage
pub use mdstream::{self, StreamError};

/// Braille spinner animation frames.
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// A [`Writer`] that wraps [`TermWriter`] and displays a spinner on the next
/// line.
///
/// This writer integrates with `mdstream`'s progressive rendering to show
/// markdown content with a spinner indicator below it during streaming.
pub struct SpinnerWriter {
    inner: TermWriter<io::Stdout>,
    output: io::Stdout,
    message: String,
    hint: &'static str,
    frame: usize,
    visible: bool,
    stopwatch: Stopwatch,
}

impl SpinnerWriter {
    /// Creates a new `SpinnerWriter` with the given message and stopwatch.
    pub fn new(message: impl Into<String>, stopwatch: Stopwatch) -> Self {
        Self {
            inner: TermWriter::stdout(),
            output: io::stdout(),
            message: message.into(),
            hint: "Ctrl+C to interrupt",
            frame: 0,
            visible: false,
            stopwatch,
        }
    }

    /// Updates the spinner message.
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    /// Returns the current stopwatch reference for time tracking.
    pub fn stopwatch(&self) -> &Stopwatch {
        &self.stopwatch
    }

    /// Clears the spinner permanently.
    pub fn finish(&mut self) {
        self.hide();
    }

    fn current_col(&self) -> u16 {
        position().map_or(0, |(col, _)| col)
    }

    fn next_frame(&mut self) -> char {
        let frame = SPINNER_FRAMES[self.frame % SPINNER_FRAMES.len()];
        self.frame = self.frame.wrapping_add(1);
        frame
    }

    fn hide(&mut self) {
        if !self.visible {
            return;
        }
        let col = self.current_col();
        let _ = writeln!(self.output);
        let _ = execute!(
            self.output,
            MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            MoveUp(1),
            MoveToColumn(col)
        );
        let _ = self.output.flush();
        self.visible = false;
    }

    fn show(&mut self) {
        let col = self.current_col();
        let frame = self.next_frame();

        let _ = writeln!(self.output);
        let _ = execute!(self.output, MoveToColumn(0), Clear(ClearType::CurrentLine));

        // Render: ⠋ Message 00s · Hint
        let _ = execute!(self.output, SetForegroundColor(Color::Green));
        let _ = write!(self.output, "{}", frame);
        let _ = execute!(self.output, SetAttribute(Attribute::Bold));
        let _ = write!(self.output, " {}", self.message);
        let _ = execute!(self.output, ResetColor, SetAttribute(Attribute::Reset));
        let _ = write!(self.output, " {}", self.stopwatch);
        let _ = execute!(
            self.output,
            SetForegroundColor(Color::White),
            SetAttribute(Attribute::Dim)
        );
        let _ = write!(self.output, " · {}", self.hint);
        let _ = execute!(self.output, ResetColor, SetAttribute(Attribute::Reset));

        let _ = execute!(self.output, MoveUp(1), MoveToColumn(col));
        let _ = self.output.flush();
        self.visible = true;
    }
}

impl Drop for SpinnerWriter {
    fn drop(&mut self) {
        self.hide();
    }
}

impl Writer for SpinnerWriter {
    fn write(&mut self, content: &str) -> Result<(), StreamError> {
        self.inner.write(content)?;
        let _ = execute!(self.output, Clear(ClearType::UntilNewLine));
        self.show();
        Ok(())
    }

    fn save_position(&mut self) -> Result<Position, StreamError> {
        self.inner.save_position()
    }

    fn replace(&mut self, position: Position, content: &str) -> Result<bool, StreamError> {
        self.hide();
        let replaced = self.inner.replace(position, content)?;
        let _ = execute!(self.output, Clear(ClearType::FromCursorDown));
        self.show();
        Ok(replaced)
    }

    fn flush(&mut self) -> Result<(), StreamError> {
        self.inner.flush()
    }
}

/// Internal markdown streamer using SpinnerWriter.
struct MarkdownStreamer {
    renderer: ProgressiveRenderer<SpinnerWriter, mdstream::formatter::TermimadFormatter>,
}

impl MarkdownStreamer {
    fn new(message: impl Into<String>, stopwatch: Stopwatch) -> Self {
        Self {
            renderer: ProgressiveRenderer::new(SpinnerWriter::new(message, stopwatch)),
        }
    }

    fn push(&mut self, token: &str) -> Result<()> {
        self.renderer.push(token)?;
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        self.renderer.writer_mut().finish();
        self.renderer.finish()?;
        Ok(())
    }
}

/// Unified spinner manager for terminal UI.
///
/// Handles both standalone spinner display and progressive markdown streaming.
#[derive(Default)]
pub struct SpinnerManager {
    /// Standalone spinner (indicatif-based, for loading states)
    spinner: Option<ProgressBar>,
    stopwatch: Stopwatch,
    message: Option<String>,
    tracker: Option<JoinHandle<()>>,
    word_index: Option<usize>,
    /// Active markdown streamer (when streaming content)
    md_streamer: Option<MarkdownStreamer>,
    /// Whether we're currently streaming reasoning content
    in_reasoning: bool,
}

impl SpinnerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start the spinner with a message.
    pub fn start(&mut self, message: Option<&str>) -> Result<()> {
        self.stop(None)?;

        let words = [
            "Thinking",
            "Processing",
            "Analyzing",
            "Forging",
            "Researching",
            "Synthesizing",
            "Reasoning",
            "Contemplating",
        ];

        // Use a random word from the list, caching the index for consistency
        let word = match message {
            Some(msg) => msg,
            None => {
                let idx = *self
                    .word_index
                    .get_or_insert_with(|| rand::rng().random_range(0..words.len()));
                words[idx]
            }
        };

        // Store the base message without styling for later use with the timer
        self.message = Some(word.to_string());

        // Start the stopwatch
        self.stopwatch.start();

        // Create the spinner with indicatif
        let pb = ProgressBar::new_spinner();

        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );

        // Setting to 60ms for a smooth yet fast animation
        pb.enable_steady_tick(std::time::Duration::from_millis(60));

        // Set the initial message
        let display_message = format!(
            "{} {} {}",
            word.green().bold(),
            self.stopwatch,
            "· Ctrl+C to interrupt".white().dimmed()
        );
        pb.set_message(display_message);

        self.spinner = Some(pb);

        // Clone the necessary components for the tracker task
        let spinner_clone = self.spinner.clone();
        let message_clone = self.message.clone();
        let stopwatch = self.stopwatch;

        // Spawn tracker to keep track of time in seconds
        self.tracker = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                if let (Some(spinner), Some(message)) = (&spinner_clone, &message_clone) {
                    let updated_message = format!(
                        "{} {} {}",
                        message.green().bold(),
                        stopwatch,
                        "· Ctrl+C to interrupt".white().dimmed()
                    );
                    spinner.set_message(updated_message);
                }
            }
        }));

        Ok(())
    }

    /// Stop the active spinner if any.
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        // Finish reasoning if active
        if self.in_reasoning {
            println!();
            self.in_reasoning = false;
        }

        // Flush markdown streamer if active
        if let Some(ref mut streamer) = self.md_streamer {
            streamer.finish()?;
        }
        self.md_streamer = None;

        self.stopwatch.stop();

        if let Some(spinner) = self.spinner.take() {
            spinner.finish_and_clear();
            if let Some(msg) = message {
                println!("{msg}");
            }
        } else if let Some(msg) = message {
            println!("{msg}");
        }

        if let Some(handle) = self.tracker.take() {
            handle.abort();
        }
        self.message = None;
        Ok(())
    }

    /// Resets the stopwatch to zero.
    /// Call this when starting a completely new task/conversation.
    pub fn reset(&mut self) {
        self.stopwatch.reset();
        self.word_index = None;
    }

    /// Stops the standalone spinner if active, preparing for streaming mode.
    fn stop_standalone_spinner(&mut self) {
        if self.spinner.is_some() {
            if let Some(spinner) = self.spinner.take() {
                spinner.finish_and_clear();
            }
            if let Some(handle) = self.tracker.take() {
                handle.abort();
            }
            self.message = None;
        }
    }

    /// Push a markdown token to stream progressively.
    ///
    /// This transitions from standalone spinner to streaming mode if needed.
    pub fn push_markdown(&mut self, token: &str) -> Result<()> {
        self.stop_standalone_spinner();

        // Finish reasoning if we were in reasoning mode
        if self.in_reasoning {
            println!();
            self.in_reasoning = false;
        }

        // Lazily create markdown streamer
        if self.md_streamer.is_none() {
            self.md_streamer = Some(MarkdownStreamer::new("Streaming", self.stopwatch));
        }

        if let Some(ref mut streamer) = self.md_streamer {
            streamer.push(token)?;
        }
        Ok(())
    }

    /// Push reasoning content to stream progressively with dimmed styling.
    ///
    /// This transitions from standalone spinner to streaming mode if needed.
    /// Reasoning content is displayed in a dimmed style to differentiate from
    /// regular markdown content.
    pub fn push_reasoning(&mut self, token: &str) -> Result<()> {
        self.stop_standalone_spinner();

        // Flush any existing markdown streamer before switching to reasoning
        if let Some(ref mut streamer) = self.md_streamer {
            streamer.finish()?;
            self.md_streamer = None;
        }

        // For reasoning, we output directly with dimmed styling
        if !token.is_empty() {
            self.in_reasoning = true;
            print!("{}", token.dimmed());
            let _ = io::stdout().flush();
        }
        Ok(())
    }

    /// Finish reasoning output and add a newline.
    pub fn finish_reasoning(&mut self) -> Result<()> {
        if self.in_reasoning {
            println!();
            self.in_reasoning = false;
        }
        Ok(())
    }

    /// Write a line while preserving spinner state.
    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        // Flush any active markdown streaming
        if let Some(ref mut streamer) = self.md_streamer {
            streamer.finish()?;
        }
        self.md_streamer = None;

        let msg = message.to_string();

        if let Some(spinner) = &self.spinner {
            spinner.suspend(|| println!("{msg}"));
        } else {
            println!("{msg}");
        }
        Ok(())
    }

    /// Write a line to stderr while preserving spinner state.
    pub fn ewrite_ln(&mut self, message: impl ToString) -> Result<()> {
        // Flush any active markdown streaming
        if let Some(ref mut streamer) = self.md_streamer {
            streamer.finish()?;
        }
        self.md_streamer = None;

        let msg = message.to_string();

        if let Some(spinner) = &self.spinner {
            spinner.suspend(|| eprintln!("{msg}"));
        } else {
            eprintln!("{msg}");
        }
        Ok(())
    }
}

impl Drop for SpinnerManager {
    fn drop(&mut self) {
        // Flush both stdout and stderr to ensure all output is visible
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn spinner_manager_new_creates_default() {
        let spinner = SpinnerManager::new();
        assert!(spinner.spinner.is_none());
        assert!(spinner.md_streamer.is_none());
    }

    #[test]
    fn spinner_manager_reset_clears_stopwatch() {
        let mut spinner = SpinnerManager::new();
        spinner.stopwatch.start();
        spinner.reset();
        assert_eq!(spinner.stopwatch.elapsed(), std::time::Duration::ZERO);
    }

    #[test]
    fn spinner_writer_new_creates_instance() {
        let stopwatch = Stopwatch::default();
        let writer = SpinnerWriter::new("Test", stopwatch);
        assert_eq!(writer.message, "Test");
        assert!(!writer.visible);
    }

    #[test]
    fn spinner_writer_set_message() {
        let stopwatch = Stopwatch::default();
        let mut writer = SpinnerWriter::new("Initial", stopwatch);
        writer.set_message("Updated");
        assert_eq!(writer.message, "Updated");
    }
}
