use std::fmt;
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

impl fmt::Display for Stopwatch {
    /// Format elapsed time as "01s", "02s", ... "59s", "1:01m", "1:59m",
    /// "1:01h", "2:30h"
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total_seconds = self.elapsed().as_secs();
        if total_seconds < 60 {
            // Less than 1 minute: "01s", "02s", etc.
            write!(f, "{:02}s", total_seconds)
        } else if total_seconds < 3600 {
            // Less than 1 hour: "1:01m", "1:59m", etc.
            let minutes = total_seconds / 60;
            let seconds = total_seconds % 60;
            write!(f, "{}:{:02}m", minutes, seconds)
        } else {
            // 1 hour or more: "1:01h", "2:30h", etc.
            let hours = total_seconds / 3600;
            let minutes = (total_seconds % 3600) / 60;
            write!(f, "{}:{:02}h", hours, minutes)
        }
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

    #[test]
    fn test_display_formats_seconds_with_leading_zero() {
        let fixture = Stopwatch { started_at: None, elapsed: std::time::Duration::from_secs(1) };
        let actual = format!("{}", fixture);
        let expected = "01s";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_display_formats_seconds_without_leading_zero() {
        let fixture = Stopwatch {
            started_at: None,
            elapsed: std::time::Duration::from_secs(30),
        };
        let actual = format!("{}", fixture);
        let expected = "30s";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_display_formats_minutes_with_seconds() {
        let fixture = Stopwatch {
            started_at: None,
            elapsed: std::time::Duration::from_secs(61),
        };
        let actual = format!("{}", fixture);
        let expected = "1:01m";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_display_formats_minutes_with_double_digit_seconds() {
        let fixture = Stopwatch {
            started_at: None,
            elapsed: std::time::Duration::from_secs(80),
        };
        let actual = format!("{}", fixture);
        let expected = "1:20m";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_display_formats_hours_with_minutes() {
        let fixture = Stopwatch {
            started_at: None,
            elapsed: std::time::Duration::from_secs(3600),
        };
        let actual = format!("{}", fixture);
        let expected = "1:00h";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_display_formats_hours_with_non_zero_minutes() {
        let fixture = Stopwatch {
            started_at: None,
            elapsed: std::time::Duration::from_secs(9000),
        };
        let actual = format!("{}", fixture);
        let expected = "2:30h";
        assert_eq!(actual, expected);
    }
}
