use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::IndexedRandom;
use tokio::task::JoinHandle;

/// Trait for spinner functionality to enable testing
pub trait Spinner: Send + Sync {
    /// Start the spinner with an optional message
    fn start(&mut self, message: Option<&str>) -> Result<()>;
    /// Stop and clear the spinner
    fn stop(&mut self) -> Result<()>;
    /// Check if spinner is currently running
    fn is_running(&self) -> bool;
}

/// Default spinner implementation
pub struct ForgeSpinner {
    spinner: Option<ProgressBar>,
    start_time: Option<Instant>,
    tracker: Option<JoinHandle<()>>,
}

impl ForgeSpinner {
    pub fn new() -> Self {
        Self { spinner: None, start_time: None, tracker: None }
    }
}

impl Default for ForgeSpinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner for ForgeSpinner {
    fn start(&mut self, message: Option<&str>) -> Result<()> {
        self.stop()?;

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

        // Initialize the start time for the timer
        self.start_time = Some(Instant::now());

        // Create the spinner with a better style that respects terminal width
        let pb = ProgressBar::new_spinner();

        // This style includes {msg} which will be replaced with our formatted message
        // The {spinner} will show a visual spinner animation
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );

        // Increase the tick rate to make the spinner move faster
        // Setting to 60ms for a smooth yet fast animation
        pb.enable_steady_tick(std::time::Duration::from_millis(60));

        // Set the initial message
        let message = format!(
            "{} 0s · {}",
            word.green().bold(),
            "Ctrl+C to interrupt".white().dimmed()
        );
        pb.set_message(message);

        self.spinner = Some(pb);

        // Clone the necessary components for the tracker task
        let spinner_clone = self.spinner.clone();
        let start_time_clone = self.start_time;
        let message_clone = word.to_string();

        // Spwan tracker to keep the track of time in sec.
        self.tracker = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                // Update the spinner with the current elapsed time
                if let (Some(spinner), Some(start_time)) = (&spinner_clone, start_time_clone) {
                    let elapsed = start_time.elapsed();
                    let seconds = elapsed.as_secs();

                    // Create a new message with the elapsed time
                    let updated_message = format!(
                        "{} {}s · {}",
                        message_clone.green().bold(),
                        seconds,
                        "Ctrl+C to interrupt".white().dimmed()
                    );

                    // Update the spinner's message
                    spinner.set_message(updated_message);
                }
            }
        }));

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_and_clear();
        }

        // Stop the tracker task
        if let Some(tracker) = self.tracker.take() {
            drop(tracker);
        }

        self.start_time = None;
        self.tracker = None;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.spinner.is_some()
    }
}
