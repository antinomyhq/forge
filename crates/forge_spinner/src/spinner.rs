//! A simplified spinner implementation using indicatif's built-in features.
//!
//! This module provides a cleaner spinner that leverages indicatif's native
//! elapsed time tracking and automatic redraw capabilities, eliminating the
//! need for manual background tasks.

use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
use rand::Rng;

const SPINNER_CHARS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

const THINKING_WORDS: &[&str] = &[
    "Thinking",
    "Processing",
    "Analyzing",
    "Forging",
    "Researching",
    "Synthesizing",
    "Reasoning",
    "Contemplating",
];

const TICK_INTERVAL: Duration = Duration::from_millis(60);
const DEFAULT_HINT: &str = "· Ctrl+C to interrupt";

/// Formats elapsed time as "01s", "1:01m", "1:01h".
fn format_elapsed(state: &ProgressState, w: &mut dyn std::fmt::Write) {
    let total_seconds = state.elapsed().as_secs();
    let formatted = if total_seconds < 60 {
        format!("{:02}s", total_seconds)
    } else if total_seconds < 3600 {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{}:{:02}m", minutes, seconds)
    } else {
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        format!("{}:{:02}h", hours, minutes)
    };
    let _ = w.write_str(&formatted);
}

/// A spinner that displays progress with elapsed time using indicatif's
/// built-in features.
///
/// Uses indicatif's native capabilities:
/// - `enable_steady_tick` for automatic redraw (no background task needed)
/// - `suspend` for writing during spinner activity
/// - Custom elapsed time format via `with_key`: "01s", "1:01m", "1:01h"
#[derive(Default)]
pub struct Spinner {
    progress_bar: Option<ProgressBar>,
    word_index: Option<usize>,
    paused: bool,
    current_message: Option<String>,
}

impl Spinner {
    /// Starts the spinner with an optional custom message.
    ///
    /// If no message is provided, a random "thinking" word is selected and
    /// cached for consistency across restarts within the same session.
    pub fn start(&mut self, message: Option<&str>) -> Result<()> {
        self.stop(None)?;

        let word = match message {
            Some(msg) => msg.to_string(),
            None => {
                let idx = *self
                    .word_index
                    .get_or_insert_with(|| rand::rng().random_range(0..THINKING_WORDS.len()));
                THINKING_WORDS[idx].to_string()
            }
        };

        self.current_message = Some(word.clone());

        let pb = ProgressBar::new_spinner();
        pb.set_style(Self::create_style());
        pb.set_message(word.green().bold().to_string());
        pb.set_prefix(DEFAULT_HINT);
        pb.enable_steady_tick(TICK_INTERVAL);

        self.progress_bar = Some(pb);
        self.paused = false;

        Ok(())
    }

    /// Stops the active spinner if any.
    ///
    /// Optionally prints a final message after clearing the spinner.
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        if let Some(pb) = self.progress_bar.take() {
            pb.finish_and_clear();
            if let Some(msg) = message {
                println!("{msg}");
            }
        } else if let Some(msg) = message {
            println!("{msg}");
        }

        self.paused = false;
        self.current_message = None;

        Ok(())
    }

    /// Pauses the spinner, clearing it from the terminal.
    ///
    /// The elapsed time continues to accumulate while paused.
    /// Use `resume()` to show the spinner again.
    pub fn pause(&mut self) -> Result<()> {
        if let Some(pb) = &self.progress_bar
            && !self.paused
        {
            let elapsed = pb.elapsed();
            let message = self.current_message.clone().unwrap_or_default();
            pb.finish_and_clear();

            let mut new_pb = ProgressBar::new_spinner();
            new_pb.set_draw_target(ProgressDrawTarget::hidden());
            new_pb = new_pb.with_elapsed(elapsed);
            new_pb.set_style(Self::create_style());
            new_pb.set_message(message.green().bold().to_string());
            new_pb.set_prefix(DEFAULT_HINT);

            self.progress_bar = Some(new_pb);
            self.paused = true;
        }
        Ok(())
    }

    /// Resumes a paused spinner, showing it in the terminal again.
    ///
    /// Has no effect if the spinner is not paused or not active.
    pub fn resume(&mut self) -> Result<()> {
        if let Some(pb) = &self.progress_bar
            && self.paused
        {
            pb.set_draw_target(ProgressDrawTarget::stderr());
            pb.enable_steady_tick(TICK_INTERVAL);
            self.paused = false;
        }
        Ok(())
    }

    /// Updates the spinner's displayed message.
    ///
    /// The message is styled with green bold formatting.
    pub fn set_message(&mut self, message: &str) -> Result<()> {
        if let Some(pb) = &self.progress_bar {
            self.current_message = Some(message.to_owned());
            pb.set_message(message.green().bold().to_string());
        }
        Ok(())
    }

    /// Resets the spinner state for a new task/conversation.
    ///
    /// Clears the cached word index so a new random word will be selected.
    pub fn reset(&mut self) {
        self.word_index = None;
        self.paused = false;
        self.current_message = None;
    }

    /// Writes a line to stdout while the spinner is active.
    ///
    /// Uses indicatif's suspend feature to temporarily hide the spinner,
    /// print the message, then restore the spinner.
    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        let msg = message.to_string();
        if let Some(pb) = &self.progress_bar {
            pb.suspend(|| println!("{msg}"));
        } else {
            println!("{msg}");
        }
        Ok(())
    }

    /// Writes a line to stderr while the spinner is active.
    ///
    /// Uses indicatif's suspend feature to temporarily hide the spinner,
    /// print the message, then restore the spinner.
    pub fn ewrite_ln(&mut self, message: impl ToString) -> Result<()> {
        let msg = message.to_string();
        if let Some(pb) = &self.progress_bar {
            pb.suspend(|| eprintln!("{msg}"));
        } else {
            eprintln!("{msg}");
        }
        Ok(())
    }

    /// Returns whether the spinner is currently active (started and not
    /// stopped).
    pub fn is_active(&self) -> bool {
        self.progress_bar.is_some()
    }

    /// Creates the default progress style for the spinner.
    fn create_style() -> ProgressStyle {
        ProgressStyle::default_spinner()
            .tick_strings(SPINNER_CHARS)
            .with_key("my_elapsed", format_elapsed)
            .template("{spinner:.green} {msg} {my_elapsed:.white} {prefix:.white.dim}")
            .expect("Invalid template")
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if let Some(pb) = self.progress_bar.take() {
            pb.finish_and_clear();
        }
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_spinner_new_creates_inactive_spinner() {
        let fixture = Spinner::default();

        let actual = fixture.is_active();
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_spinner_start_activates_spinner() {
        let mut fixture = Spinner::default();

        fixture.start(Some("Test")).unwrap();
        let actual = fixture.is_active();
        fixture.stop(None).unwrap();

        let expected = true;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_spinner_stop_deactivates_spinner() {
        let mut fixture = Spinner::default();

        fixture.start(Some("Test")).unwrap();
        fixture.stop(None).unwrap();

        let actual = fixture.is_active();
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_word_index_caching_behavior() {
        let mut fixture = Spinner::default();

        fixture.start(None).unwrap();
        let first_index = fixture.word_index;
        fixture.stop(None).unwrap();

        fixture.start(None).unwrap();
        let second_index = fixture.word_index;
        fixture.stop(None).unwrap();

        assert_eq!(first_index, second_index);
    }

    #[test]
    fn test_reset_clears_word_index() {
        let mut fixture = Spinner::default();
        fixture.word_index = Some(5);

        fixture.reset();

        let actual = fixture.word_index;
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_double_stop_is_safe() {
        let mut fixture = Spinner::default();

        fixture.start(Some("Test")).unwrap();
        fixture.stop(None).unwrap();
        let result = fixture.stop(None);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_start_stops_existing_spinner() {
        let mut fixture = Spinner::default();

        fixture.start(Some("First")).unwrap();
        let result = fixture.start(Some("Second"));

        fixture.stop(None).unwrap();

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pause_and_resume() {
        let mut fixture = Spinner::default();

        fixture.start(Some("Test")).unwrap();
        fixture.pause().unwrap();

        assert!(fixture.paused);
        assert!(fixture.is_active());

        fixture.resume().unwrap();

        assert!(!fixture.paused);
        assert!(fixture.is_active());

        fixture.stop(None).unwrap();
    }

    #[tokio::test]
    async fn test_pause_on_inactive_spinner_is_safe() {
        let mut fixture = Spinner::default();

        let result = fixture.pause();

        assert!(result.is_ok());
        assert!(!fixture.paused);
    }

    #[tokio::test]
    async fn test_resume_on_inactive_spinner_is_safe() {
        let mut fixture = Spinner::default();

        let result = fixture.resume();

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_clears_paused_state() {
        let mut fixture = Spinner::default();

        fixture.start(Some("Test")).unwrap();
        fixture.pause().unwrap();
        fixture.stop(None).unwrap();

        let actual = fixture.paused;
        let expected = false;

        assert_eq!(actual, expected);
    }
}
