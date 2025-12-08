use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

/// Manages determinate progress bar for operations with known total
#[derive(Default)]
pub struct ProgressBarManager {
    bar: Option<ProgressBar>,
}

impl ProgressBarManager {
    /// Starts a progress bar with a known total
    pub fn start(&mut self, total: u64, message: &str) -> Result<()> {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} {elapsed} {msg:.green} {bar:20.green} [{pos}/{len}]",
            )
            .unwrap()
            .progress_chars("█░░")
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.enable_steady_tick(std::time::Duration::from_millis(60));
        pb.set_message(message.to_string());
        self.bar = Some(pb);
        Ok(())
    }

    /// Updates the progress bar position
    pub fn set_position(&self, current: u64) -> Result<()> {
        if let Some(bar) = &self.bar {
            bar.set_position(current);
        }
        Ok(())
    }

    /// Updates the progress bar message
    pub fn set_message(&self, message: &str) -> Result<()> {
        if let Some(bar) = &self.bar {
            bar.set_message(message.to_string());
        }
        Ok(())
    }

    /// Stops the progress bar and optionally prints a message
    pub async fn stop(&mut self, message: Option<String>) -> Result<()> {
        if let Some(bar) = self.bar.take() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            bar.finish_and_clear();
            if let Some(msg) = message {
                println!("{msg}");
            }
        } else if let Some(msg) = message {
            println!("{msg}");
        }
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.bar.as_ref().is_some_and(|bar| !bar.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_creates_inactive() {
        let manager = ProgressBarManager::default();
        assert!(manager.bar.is_none());
    }
}
