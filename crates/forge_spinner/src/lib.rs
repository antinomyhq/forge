use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;

/// Manages spinner functionality for the UI
#[derive(Default)]
pub struct SpinnerManager {
    spinner: Option<ProgressBar>,
    start_time: Option<Instant>,
    message: Option<String>,
}

impl SpinnerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start the spinner with a message
    pub fn start(&mut self) -> Result<()> {
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
        let word = words.choose(&mut rand::thread_rng()).unwrap_or(&words[0]);

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

        Ok(())
    }

    /// Update the spinner with the current elapsed time
    pub fn update_time(&mut self) -> Result<()> {
        if let (Some(start_time), Some(message), Some(spinner)) =
            (self.start_time, self.message.as_ref(), &mut self.spinner)
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
            // No need to call tick() as we're using enable_steady_tick
            spinner.set_message(updated_message);
        }

        Ok(())
    }

    /// Stop the active spinner if any
    pub fn stop(&mut self, message: Option<String>) -> Result<()> {
        if let Some(spinner) = self.spinner.take() {
            // Always finish the spinner first
            spinner.finish_and_clear();

            // Then print the message if provided
            if let Some(msg) = message {
                println!("{}", msg);
            }
        } else if let Some(message) = message {
            // If there's no spinner but we have a message, just print it
            println!("{}", message);
        }

        self.start_time = None;
        self.message = None;
        Ok(())
    }
}
