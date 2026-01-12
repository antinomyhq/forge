use std::sync::{Arc, Mutex};

use anyhow::Result;
use colored::Colorize;
use forge_domain::ConsoleWriter;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use tokio::task::JoinHandle;

mod progress_bar;
mod stopwatch;

pub use progress_bar::*;
use stopwatch::Stopwatch;

/// Manages spinner functionality for the UI.
pub struct SpinnerManager<P: ConsoleWriter> {
    spinner: Option<ProgressBar>,
    stopwatch: Stopwatch,
    message: Option<String>,
    tracker: Arc<Mutex<Option<JoinHandle<()>>>>,
    word_index: Option<usize>,
    printer: Arc<P>,
    #[cfg(test)]
    tick_counter: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
}

impl<P: ConsoleWriter> SpinnerManager<P> {
    /// Creates a new SpinnerManager with the given output printer.
    pub fn new(printer: Arc<P>) -> Self {
        Self {
            spinner: None,
            stopwatch: Stopwatch::default(),
            message: None,
            tracker: Arc::new(Mutex::new(None)),
            word_index: None,
            printer,
            #[cfg(test)]
            tick_counter: None,
        }
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
        let message = format!(
            "{} {} {}",
            word.green().bold(),
            self.stopwatch,
            "· Ctrl+C to interrupt".white().dimmed()
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
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                #[cfg(test)]
                if let Some(counter) = &tick_counter_clone {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
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
        });
        *self.tracker.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);

        Ok(())
    }

    /// Stop the active spinner if any
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        self.stopwatch.stop();

        if let Some(spinner) = self.spinner.take() {
            spinner.finish_and_clear();
            if let Some(msg) = message {
                self.println(&msg);
            }
        } else if let Some(message) = message {
            self.println(&message);
        }

        if let Some(handle) = self
            .tracker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
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

    /// Writes a line to stdout, suspending the spinner if active.
    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        let msg = message.to_string();
        if let Some(spinner) = &self.spinner {
            let printer = self.printer.clone();
            spinner.suspend(|| {
                let line = format!("{msg}\n");
                let _ = printer.write(line.as_bytes());
                let _ = printer.flush();
            });
        } else {
            self.println(&msg);
        }
        Ok(())
    }

    /// Writes a line to stderr, suspending the spinner if active.
    pub fn ewrite_ln(&mut self, message: impl ToString) -> Result<()> {
        let msg = message.to_string();
        if let Some(spinner) = &self.spinner {
            let printer = self.printer.clone();
            spinner.suspend(|| {
                let line = format!("{msg}\n");
                let _ = printer.write_err(line.as_bytes());
                let _ = printer.flush_err();
            });
        } else {
            self.eprintln(&msg);
        }
        Ok(())
    }

    /// Prints a line to stdout through the printer.
    fn println(&self, msg: &str) {
        let line = format!("{msg}\n");
        let _ = self.printer.write(line.as_bytes());
        let _ = self.printer.flush();
    }

    /// Prints a line to stderr through the printer.
    fn eprintln(&self, msg: &str) {
        let line = format!("{msg}\n");
        let _ = self.printer.write_err(line.as_bytes());
        let _ = self.printer.flush_err();
    }
}

impl<P: ConsoleWriter> Drop for SpinnerManager<P> {
    fn drop(&mut self) {
        // Flush both stdout and stderr to ensure all output is visible
        // This prevents race conditions with shell prompt resets
        let _ = self.printer.flush();
        let _ = self.printer.flush_err();
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use forge_domain::ConsoleWriter;
    use pretty_assertions::assert_eq;

    use super::SpinnerManager;

    /// A simple printer that writes directly to stdout/stderr.
    /// Used for testing when synchronized output is not needed.
    #[derive(Clone, Copy)]
    struct DirectPrinter;

    impl ConsoleWriter for DirectPrinter {
        fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
            std::io::stdout().write(buf)
        }

        fn write_err(&self, buf: &[u8]) -> std::io::Result<usize> {
            std::io::stderr().write(buf)
        }

        fn flush(&self) -> std::io::Result<()> {
            std::io::stdout().flush()
        }

        fn flush_err(&self) -> std::io::Result<()> {
            std::io::stderr().flush()
        }
    }

    fn fixture_spinner() -> SpinnerManager<DirectPrinter> {
        SpinnerManager::new(Arc::new(DirectPrinter))
    }

    fn fixture_spinner_with_counter(counter: Arc<AtomicU64>) -> SpinnerManager<DirectPrinter> {
        SpinnerManager {
            spinner: None,
            stopwatch: Default::default(),
            message: None,
            tracker: Arc::new(std::sync::Mutex::new(None)),
            word_index: None,
            printer: Arc::new(DirectPrinter),
            tick_counter: Some(counter),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_spinner_tracker_task_is_stopped_on_stop() {
        let fixture_counter = Arc::new(AtomicU64::new(0));
        let mut fixture_spinner = fixture_spinner_with_counter(fixture_counter.clone());

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
        let mut fixture_spinner = fixture_spinner();

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

    #[tokio::test]
    async fn test_word_index_caching_behavior() {
        let mut fixture_spinner = fixture_spinner();

        // Start spinner without message multiple times
        fixture_spinner.start(None).unwrap();
        let first_message = fixture_spinner.message.clone();
        fixture_spinner.stop(None).unwrap();

        fixture_spinner.start(None).unwrap();
        let second_message = fixture_spinner.message.clone();
        fixture_spinner.stop(None).unwrap();

        // Messages should be identical because word_index is cached
        assert_eq!(first_message, second_message);
    }
}
