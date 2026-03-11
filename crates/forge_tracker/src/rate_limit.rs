use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use chrono::Utc;

/// A simple thread-safe rate limiter that allows a maximum number of events per
/// minute.
#[derive(Debug)]
pub struct RateLimiter {
    max_per_minute: usize,
    window_start: AtomicU64,
    count: AtomicUsize,
}

impl RateLimiter {
    /// Creates a new rate limiter with the specified maximum events per minute.
    pub fn new(max_per_minute: usize) -> Self {
        Self {
            max_per_minute,
            window_start: AtomicU64::new(Utc::now().timestamp() as u64),
            count: AtomicUsize::new(0),
        }
    }

    /// Checks if an event should be allowed based on the rate limit.
    /// Returns true if the event is allowed, false if it should be dropped.
    pub fn check(&self) -> bool {
        let now = Utc::now().timestamp() as u64;
        let window_start = self.window_start.load(Ordering::Relaxed);

        // If a minute has passed, reset the window and counter
        if now.saturating_sub(window_start) >= 60 {
            // We use compare_exchange to avoid race conditions when multiple threads try to
            // reset
            if self
                .window_start
                .compare_exchange(window_start, now, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                self.count.store(0, Ordering::SeqCst);
            }
        }

        // Increment and check the rate limit
        self.count.fetch_add(1, Ordering::Relaxed) < self.max_per_minute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(2);

        assert!(limiter.check()); // 1
        assert!(limiter.check()); // 2
        assert!(!limiter.check()); // 3 - blocked
        assert!(!limiter.check()); // 4 - blocked
    }
}
