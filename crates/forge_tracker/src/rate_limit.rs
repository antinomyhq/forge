use std::sync::Mutex;

use chrono::Utc;

/// Thread-safe fixed-window limiter for event dispatch.
#[derive(Debug)]
pub struct RateLimiter {
    max_per_minute: usize,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    window_start: u64,
    count: usize,
}

impl RateLimiter {
    /// Creates a new rate limiter.
    ///
    /// # Arguments
    /// - `max_per_minute`: Maximum number of allowed events in each 60-second window.
    pub fn new(max_per_minute: usize) -> Self {
        Self {
            max_per_minute,
            state: Mutex::new(State {
                window_start: Utc::now().timestamp() as u64,
                count: 0,
            }),
        }
    }

    /// Checks whether a new event is allowed in the current minute window.
    ///
    /// Returns `true` when the event can be dispatched and `false` when it
    /// should be dropped.
    pub fn check(&self) -> bool {
        self.check_at(Utc::now().timestamp() as u64)
    }

    fn check_at(&self, now: u64) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        if now.saturating_sub(state.window_start) >= 60 {
            state.window_start = now;
            state.count = 0;
        }

        if state.count >= self.max_per_minute {
            return false;
        }

        state.count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_blocks_after_limit() {
        let fixture = RateLimiter::new(2);

        let actual = vec![
            fixture.check_at(100),
            fixture.check_at(100),
            fixture.check_at(100),
            fixture.check_at(100),
        ];

        let expected = vec![true, true, false, false];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_rate_limiter_resets_on_new_window() {
        let fixture = RateLimiter::new(2);
        let start = fixture
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .window_start;

        let actual = vec![
            fixture.check_at(start),
            fixture.check_at(start),
            fixture.check_at(start),
            fixture.check_at(start + 61),
            fixture.check_at(start + 61),
            fixture.check_at(start + 61),
        ];

        let expected = vec![true, true, false, true, true, false];
        assert_eq!(actual, expected);
    }
}
