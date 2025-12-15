use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::IndexedRandom;
use tokio::task::JoinHandle;

mod progress_bar;
mod stopwatch;

pub use progress_bar::*;
use stopwatch::Stopwatch;

/// Manages spinner functionality for the UI
pub struct SpinnerManager {
    spinner: Option<ProgressBar>,
    stopwatch: Stopwatch,
    message: Option<String>,
    tracker: Option<JoinHandle<()>>,
    #[cfg(test)]
    tick_counter: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
}

impl Default for SpinnerManager {
    fn default() -> Self {
        Self {
            spinner: None,
            stopwatch: Stopwatch::default(),
            message: None,
            tracker: None,
            #[cfg(test)]
            tick_counter: None,
        }
    }
}

impl SpinnerManager {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn test_with_tick_counter(
        tick_counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self { tick_counter: Some(tick_counter), ..Self::default() }
    }

    /// Start the spinner with a message
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

        // Use a random word from the list
        let word = match message {
            None => words.choose(&mut rand::rng()).unwrap_or(&words[0]),
            Some(msg) => msg,
        };

        // Store the base message without styling for later use with the timer
        self.message = Some(word.to_string());

        // Start the stopwatch
        self.stopwatch.start();

        // Create the spinner with a better style that respects terminal width
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
        let elapsed = Duration::from_secs(self.stopwatch.elapsed().as_secs());
        let message = format!(
            "{} {} · {}",
            word.green().bold(),
            humantime::format_duration(elapsed),
            "Ctrl+C to interrupt".white().dimmed()
        );
        pb.set_message(message);

        self.spinner = Some(pb);

        // Clone the necessary components for the tracker task
        let spinner_clone = self.spinner.clone();
        let message_clone = self.message.clone();
        let stopwatch = self.stopwatch;
        #[cfg(test)]
        let tick_counter_clone = self.tick_counter.clone();

        // Spawn tracker to keep track of time in seconds
        self.tracker = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                #[cfg(test)]
                if let Some(counter) = &tick_counter_clone {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                if let (Some(spinner), Some(message)) = (&spinner_clone, &message_clone) {
                    let elapsed = Duration::from_secs(stopwatch.elapsed().as_secs());
                    let updated_message = format!(
                        "{} {} · {}",
                        message.green().bold(),
                        humantime::format_duration(elapsed),
                        "Ctrl+C to interrupt".white().dimmed()
                    );
                    spinner.set_message(updated_message);
                }
            }
        }));

        Ok(())
    }

    /// Stop the active spinner if any
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        self.stop_inner(message, |s| println!("{s}"))
    }

    /// Resets the stopwatch to zero.
    /// Call this when starting a completely new task/conversation.
    pub fn reset(&mut self) {
        self.stopwatch.reset();
    }

    fn stop_inner<F>(&mut self, message: Option<String>, writer: F) -> Result<()>
    where
        F: FnOnce(&str),
    {
        self.stopwatch.stop();

        if let Some(spinner) = self.spinner.take() {
            spinner.finish_and_clear();
            if let Some(msg) = message {
                writer(&msg);
            }
        } else if let Some(message) = message {
            writer(&message);
        }

        if let Some(handle) = self.tracker.take() {
            handle.abort();
        }
        self.message = None;
        Ok(())
    }

    fn write_with_restart<F>(&mut self, message: impl ToString, writer: F) -> Result<()>
    where
        F: FnOnce(&str),
    {
        let is_running = self.spinner.is_some();
        let prev_message = self.message.clone();
        self.stop_inner(Some(message.to_string()), writer)?;
        if is_running {
            self.start(prev_message.as_deref())?
        }
        Ok(())
    }

    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        self.write_with_restart(message, |msg| println!("{msg}"))
    }

    pub fn ewrite_ln(&mut self, message: impl ToString) -> Result<()> {
        self.write_with_restart(message, |msg| eprintln!("{msg}"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use pretty_assertions::assert_eq;

    use super::SpinnerManager;

    #[tokio::test(start_paused = true)]
    async fn test_spinner_tracker_task_is_stopped_on_stop() {
        let fixture_counter = Arc::new(AtomicU64::new(0));
        let mut fixture_spinner = SpinnerManager::test_with_tick_counter(fixture_counter.clone());

        fixture_spinner.start(Some("Test")).unwrap();
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        let actual_before_stop = fixture_counter.load(Ordering::SeqCst);
        assert!(actual_before_stop > 0);

        fixture_spinner.stop(None).unwrap();
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        let actual_after_stop = fixture_counter.load(Ordering::SeqCst);
        let expected = actual_before_stop;
        assert_eq!(actual_after_stop, expected);
    }

    #[tokio::test(start_paused = true)]
    async fn test_spinner_time_accumulates_and_resets() {
        let mut fixture_spinner = SpinnerManager::new();

        // First session
        fixture_spinner.start(Some("Test")).unwrap();
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        fixture_spinner.stop(None).unwrap();

        // Second session - time should accumulate
        fixture_spinner.start(Some("Test")).unwrap();
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        fixture_spinner.stop(None).unwrap();

        let actual_accumulated = fixture_spinner.stopwatch.elapsed();
        assert!(actual_accumulated.as_millis() >= 200);

        // Reset should clear accumulated time
        fixture_spinner.reset();

        let actual_after_reset = fixture_spinner.stopwatch.elapsed();
        let expected = std::time::Duration::ZERO;
        assert_eq!(actual_after_reset, expected);
    }
}
