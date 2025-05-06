use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Manages spinner functionality for the UI
#[derive(Default)]
pub struct SpinnerManager {
    spinner: Option<ProgressBar>,
    start_time: Option<Instant>,
    message: Option<String>,
    update_task: Option<JoinHandle<()>>,
    // Channel for stopping the timer task
    stop_tx: Option<mpsc::Sender<()>>,
}

impl SpinnerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start the spinner with a message
    pub fn start(&mut self, message: Option<&str>) -> Result<()> {
        // Stop any existing spinner and update task
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
            None => words.choose(&mut rand::thread_rng()).unwrap_or(&words[0]),
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

        Ok(())
    }

    /// Start the spinner with auto-updating timer
    pub fn start_with_auto_update(&mut self, message: Option<&str>) -> Result<()> {
        // First, start the regular spinner
        self.start(message)?;
        
        // Create a channel for stopping the update task
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        // Get essential spinner information
        let spinner = self.spinner.as_ref().expect("Spinner should be initialized").clone();
        let message = self.message.clone().expect("Message should be initialized");
        let start_time = self.start_time.expect("Start time should be initialized");
        
        // Create a new tokio task to update the spinner timer
        let update_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
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
                    _ = stop_rx.recv() => {
                        // Stop signal received, exit the loop
                        break;
                    }
                }
            }
        });
        
        // Store the task handle for cleanup on stop
        self.update_task = Some(update_task);
        
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
        // Send stop signal to the update task if it exists
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.try_send(());  // Ignore errors if the receiver is already dropped
        }

        // Cancel the update task if it's running
        if let Some(task) = self.update_task.take() {
            task.abort();
        }

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

// Clean up the update task when the SpinnerManager is dropped
impl Drop for SpinnerManager {
    fn drop(&mut self) {
        // Try to send stop signal first
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.try_send(());  // Ignore errors if the receiver is already dropped
        }
        
        // Then abort the task if needed
        if let Some(task) = self.update_task.take() {
            task.abort();
        }
    }
}
