use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::IndexedRandom;
use tokio::task::JoinHandle;

/// Manages spinner functionality for the UI
#[derive(Default)]
pub struct SpinnerManager {
    spinner: Option<ProgressBar>,
    start_time: Option<Instant>,
    message: Option<String>,
    tracker: Option<JoinHandle<()>>,
}

impl SpinnerManager {
    pub fn new() -> Self {
        Self::default()
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
        let message_clone = self.message.clone();

        // Spwan tracker to keep the track of time in sec.
        self.tracker = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            loop {
                interval.tick().await;
                // Update the spinner with the current elapsed time
                if let (Some(spinner), Some(start_time), Some(message)) =
                    (&spinner_clone, start_time_clone, &message_clone)
                {
                    let elapsed = start_time.elapsed();
                    let seconds = elapsed.as_secs();

                    // Create a new message with the elapsed time
                    let updated_message = format!(
                        "{} {}s · {}",
                        message.green().bold(),
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

    /// Stop the active spinner if any
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        if let Some(spinner) = self.spinner.take() {
            // Always finish the spinner first
            spinner.finish_and_clear();

            // Then print the message if provided
            if let Some(msg) = message {
                println!("{msg}");
            }
        } else if let Some(message) = message {
            // If there's no spinner but we have a message, just print it
            println!("{message}");
        }

        // Tracker task will be dropped here.
        if let Some(a) = self.tracker.take() {
            drop(a)
        }
        self.tracker = None;
        self.start_time = None;
        self.message = None;
        Ok(())
    }

    pub fn write_ln(&mut self, message: impl ToString) -> Result<()> {
        let is_running = self.spinner.is_some();
        let prev_message = self.message.clone();
        self.stop(Some(message.to_string()))?;
        if is_running {
            self.start(prev_message.as_deref())?
        }

        Ok(())
    }
}
