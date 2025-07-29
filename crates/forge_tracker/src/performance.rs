use std::time::Instant;

/// Performance timing utilities for tracking execution duration
pub struct PerformanceTimer {
    start: Instant,
    operation: String,
}

impl PerformanceTimer {
    pub fn new(operation: impl Into<String>) -> Self {
        Self { start: Instant::now(), operation: operation.into() }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }
}

/// Central timing manager for performance tracking
/// Provides a clean API for measuring operation durations
#[derive(Clone)]
pub struct TimingManager {
    start: Instant,
}

impl TimingManager {
    /// Create a new timing manager, starting the timer immediately
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    /// Get elapsed time in milliseconds since creation
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// Get elapsed time in microseconds since creation
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    /// Reset the timer to the current moment
    pub fn reset(&mut self) {
        self.start = Instant::now();
    }

    /// Create a checkpoint that returns elapsed time and resets the timer
    pub fn checkpoint(&mut self) -> u64 {
        let elapsed = self.elapsed_ms();
        self.reset();
        elapsed
    }

    /// Execute a closure and return both the result and elapsed time in
    /// milliseconds
    pub fn time<T, F>(&mut self, operation: F) -> (T, u64)
    where
        F: FnOnce() -> T,
    {
        self.reset();
        let result = operation();
        let elapsed = self.elapsed_ms();
        (result, elapsed)
    }

    /// Execute an async closure and return both the result and elapsed time in
    /// milliseconds
    pub async fn time_async<T, F, Fut>(&mut self, operation: F) -> (T, u64)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.reset();
        let result = operation().await;
        let elapsed = self.elapsed_ms();
        (result, elapsed)
    }
}

/// Convenience function to create a new timing manager
pub fn start_timing() -> TimingManager {
    TimingManager::start()
}

/// Macro for timing operations and dispatching performance events
#[macro_export]
macro_rules! trace_performance {
    ($tracker:expr, $event_constructor:expr) => {{
        let start = std::time::Instant::now();
        let result = { $event_constructor };
        let duration = start.elapsed().as_millis() as u64;
        result
    }};
}

/// Macro for timing async operations
#[macro_export]
macro_rules! trace_performance_async {
    ($tracker:expr, $event_constructor:expr, $async_operation:expr) => {{
        let start = std::time::Instant::now();
        let result = $async_operation.await;
        let duration = start.elapsed().as_millis() as u64;
        // Dispatch the event with the measured duration
        if let Ok(event) = $event_constructor(duration) {
            tokio::spawn($tracker.dispatch(event));
        }
        result
    }};
}

/// Memory tracking utilities
pub struct MemoryTracker {
    initial_usage: usize,
    location: String,
}

impl MemoryTracker {
    pub fn new(location: impl Into<String>) -> Self {
        Self { initial_usage: get_memory_usage(), location: location.into() }
    }

    pub fn current_usage(&self) -> usize {
        get_memory_usage()
    }

    pub fn usage_delta(&self) -> isize {
        get_memory_usage() as isize - self.initial_usage as isize
    }

    pub fn location(&self) -> &str {
        &self.location
    }
}

/// Get current memory usage (simplified implementation)
/// In a real implementation, you might want to use a more sophisticated
/// approach
pub fn get_memory_usage() -> usize {
    // This is a simplified implementation
    // In production, you might want to use jemalloc stats or similar
    // For now, we'll return a placeholder value
    // TODO: Implement proper memory tracking using system APIs or allocator stats
    0
}

/// Track clone operations
pub fn track_clone_operation<T, F>(operation: F) -> T
where
    F: FnOnce() -> T,
{
    let _memory_tracker = MemoryTracker::new("clone_operation");
    operation()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_performance_timer() {
        let timer = PerformanceTimer::new("test_operation");
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10);
        assert_eq!(timer.operation(), "test_operation");
    }

    #[test]
    fn test_timing_manager_basic() {
        let timer = TimingManager::start();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10);
    }

    #[test]
    fn test_timing_manager_checkpoint() {
        let mut timer = TimingManager::start();
        std::thread::sleep(Duration::from_millis(10));
        let checkpoint1 = timer.checkpoint();
        assert!(checkpoint1 >= 10);

        std::thread::sleep(Duration::from_millis(5));
        let checkpoint2 = timer.checkpoint();
        assert!(checkpoint2 >= 5);
        assert!(checkpoint2 < checkpoint1);
    }

    #[test]
    fn test_timing_manager_time_function() {
        let mut timer = TimingManager::start();
        let (result, elapsed) = timer.time(|| {
            std::thread::sleep(Duration::from_millis(10));
            42
        });
        assert_eq!(result, 42);
        assert!(elapsed >= 10);
    }

    #[tokio::test]
    async fn test_timing_manager_time_async() {
        let mut timer = TimingManager::start();
        let (result, elapsed) = timer
            .time_async(|| async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                42
            })
            .await;
        assert_eq!(result, 42);
        assert!(elapsed >= 10);
    }

    #[test]
    fn test_start_timing_convenience() {
        let timer = start_timing();
        std::thread::sleep(Duration::from_millis(5));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 5);
    }

    #[test]
    fn test_memory_tracker() {
        let tracker = MemoryTracker::new("test_location");
        assert_eq!(tracker.location(), "test_location");
        // Memory usage tests are environment-dependent, so we just test the interface
        let _current = tracker.current_usage();
        let _delta = tracker.usage_delta();
    }
}
