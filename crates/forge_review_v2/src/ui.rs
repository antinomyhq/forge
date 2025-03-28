use std::sync::Arc;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Creates and configures the progress indicators and UI elements for the CLI
/// application.
pub struct UI {
    multi_progress: Arc<MultiProgress>,
    success_prefix: &'static str,
}

impl UI {
    /// Creates a new UI instance.
    pub fn new() -> Self {
        UI {
            multi_progress: Arc::new(MultiProgress::new()),
            success_prefix: "\x1b[32m✓\x1b[0m", // Green checkmark with color reset
        }
    }

    /// Create a spinner style for in-progress tasks.
    pub fn create_spinner_style(&self) -> ProgressStyle {
        ProgressStyle::default_spinner()
            .tick_chars("⣾⣽⣻⢿⡿⣟⣯⣷")
            .template("{prefix:<2.cyan} {spinner:.blue} {wide_msg}")
            .unwrap()
    }

    /// Create a style for completed tasks.
    pub fn create_completion_style(&self) -> ProgressStyle {
        ProgressStyle::default_spinner()
            .template("{prefix:<2.cyan} {wide_msg}")
            .unwrap()
    }

    /// Create a new spinner with the given prefix and message.
    pub fn create_spinner(&self, prefix: String, message: String) -> ProgressBar {
        let spinner = self.multi_progress.add(ProgressBar::new_spinner());
        spinner.set_style(self.create_spinner_style());
        spinner.set_prefix(prefix);
        spinner.enable_steady_tick(Duration::from_millis(80));
        spinner.set_message(message);
        spinner
    }

    /// Update a spinner to show completion.
    pub fn complete_spinner(&self, spinner: &ProgressBar, message: String) {
        spinner.set_prefix(self.success_prefix);
        spinner.set_style(self.create_completion_style());
        spinner.finish_with_message(message);
    }

    /// Create a progress bar with the given length.
    pub fn create_progress_bar(&self, length: u64, message: String) -> ProgressBar {
        let progress_bar = self.multi_progress.add(ProgressBar::new(length));
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {prefix:<2.bold.dim} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} • {wide_msg}")
                .unwrap()
                .progress_chars("█▓▒░ ")
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
        );
        progress_bar.enable_steady_tick(Duration::from_millis(80));
        progress_bar.set_message(message);
        progress_bar
    }

    /// Complete a progress bar with a success style and message.
    pub fn complete_progress_bar(&self, progress_bar: &ProgressBar, message: String) {
        progress_bar.set_prefix(self.success_prefix);
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template("{prefix:<2.bold.green} {wide_msg} [{elapsed_precise}] {bar:40.bright_green} {percent}%")
                .unwrap()
                .progress_chars("█▓▒░ ")
        );
        progress_bar.finish_with_message(message);
    }
}
