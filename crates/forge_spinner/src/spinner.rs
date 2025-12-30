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

/// Formats a duration as "01s", "1:01m", "1:01h".
fn format_duration_string(total_seconds: u64) -> String {
    if total_seconds < 60 {
        format!("{:02}s", total_seconds)
    } else if total_seconds < 3600 {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{}:{:02}m", minutes, seconds)
    } else {
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        format!("{}:{:02}h", hours, minutes)
    }
}

/// Formats elapsed time as "01s", "1:01m", "1:01h".
fn format_elapsed(state: &ProgressState, w: &mut dyn std::fmt::Write) {
    let _ = w.write_str(&format_duration_string(state.elapsed().as_secs()));
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
            let idx = *self
                .word_index
                .get_or_insert_with(|| rand::rng().random_range(0..THINKING_WORDS.len()));
            let word = THINKING_WORDS[idx].to_string();
            self.current_message = Some(format!("{} {}", word, message));
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

    /// Returns whether the spinner is currently active (started, not stopped,
    /// and not paused).
    pub fn is_active(&self) -> bool {
        self.progress_bar.is_some() && !self.paused
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
    fn format_duration_seconds_only() {
        let actual = format_duration_string(45);
        assert_eq!(actual, "45s");
    }

    #[test]
    fn format_duration_minutes_and_seconds() {
        let actual = format_duration_string(125); // 2:05
        assert_eq!(actual, "2:05m");
    }

    #[test]
    fn format_duration_hours_and_minutes() {
        let actual = format_duration_string(3725); // 1:02h (3600 + 120 + 5)
        assert_eq!(actual, "1:02h");
    }

    #[test]
    fn spinner_is_inactive_by_default() {
        let spinner = Spinner::default();
        assert!(!spinner.is_active());
    }

    #[test]
    fn spinner_is_active_after_start() {
        let mut spinner = Spinner::default();
        spinner.start(None).unwrap();
        assert!(spinner.is_active());
    }

    #[test]
    fn spinner_is_inactive_after_stop() {
        let mut spinner = Spinner::default();
        spinner.start(None).unwrap();
        spinner.stop(None).unwrap();
        assert!(!spinner.is_active());
    }

    #[test]
    fn spinner_is_inactive_when_paused() {
        let mut spinner = Spinner::default();
        spinner.start(None).unwrap();
        spinner.pause().unwrap();

        assert!(!spinner.is_active());
        assert!(spinner.paused);
    }

    #[test]
    fn spinner_resume_clears_paused_flag() {
        let mut spinner = Spinner::default();
        spinner.start(None).unwrap();
        spinner.pause().unwrap();
        spinner.resume().unwrap();

        assert!(spinner.is_active());
        assert!(!spinner.paused);
    }

    #[test]
    fn spinner_reset_clears_state() {
        let mut spinner = Spinner::default();
        spinner.start(None).unwrap();
        spinner.word_index = Some(3);
        spinner.paused = true;

        spinner.reset();

        assert_eq!(spinner.word_index, None);
        assert!(!spinner.paused);
        assert_eq!(spinner.current_message, None);
    }

    #[test]
    fn spinner_start_uses_custom_message() {
        let mut spinner = Spinner::default();
        spinner.start(Some("Custom")).unwrap();

        assert_eq!(spinner.current_message, Some("Custom".to_string()));
    }

    #[test]
    fn spinner_start_caches_word_index() {
        let mut spinner = Spinner::default();
        spinner.start(None).unwrap();

        let first_index = spinner.word_index;
        assert!(first_index.is_some());

        spinner.stop(None).unwrap();
        spinner.start(None).unwrap();

        assert_eq!(spinner.word_index, first_index);
    }
}
