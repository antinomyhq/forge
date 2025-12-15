use std::time::Duration;

use tokio::time::Instant;

/// A stopwatch that tracks elapsed time, can be paused/resumed, and accumulates
/// time across runs.
#[derive(Clone, Copy)]
pub struct Stopwatch {
    started_at: Option<Instant>,
    elapsed: Duration,
}

impl Default for Stopwatch {
    fn default() -> Self {
        Self { started_at: None, elapsed: Duration::ZERO }
    }
}

impl Stopwatch {
    /// Start or resume the stopwatch
    pub fn start(&mut self) {
        if self.started_at.is_none() {
            self.started_at = Some(Instant::now());
        }
    }

    /// Stop the stopwatch and accumulate elapsed time
    pub fn stop(&mut self) {
        if let Some(started) = self.started_at.take() {
            self.elapsed += started.elapsed();
        }
    }

    /// Reset the stopwatch to zero
    pub fn reset(&mut self) {
        self.started_at = None;
        self.elapsed = Duration::ZERO;
    }

    /// Get total elapsed time (accumulated + current run if running)
    pub fn elapsed(&self) -> Duration {
        let current = self.started_at.map(|s| s.elapsed()).unwrap_or_default();
        self.elapsed + current
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::Stopwatch;

    #[tokio::test(start_paused = true)]
    async fn test_stopwatch_accumulates_only_while_running() {
        let mut fixture = Stopwatch::default();

        // First run - 100ms
        fixture.start();
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        fixture.stop();

        // Time passes while stopped - should NOT count
        tokio::time::advance(std::time::Duration::from_millis(500)).await;

        // Second run - 100ms more
        fixture.start();
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        fixture.stop();

        // Should be ~200ms, not 700ms
        let actual = fixture.elapsed();
        assert!(actual.as_millis() >= 200 && actual.as_millis() < 300);

        // Reset should clear
        fixture.reset();
        assert_eq!(fixture.elapsed(), std::time::Duration::ZERO);
    }
}
