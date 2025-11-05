/// Manages nudge timing decisions within the execution loop
pub struct Nudger<'a> {
    interval: &'a Option<usize>,
    completion_nudge: bool,
}

impl<'a> Nudger<'a> {
    /// Creates a new nudger
    pub fn new(interval: &'a Option<usize>) -> Self {
        Self { interval, completion_nudge: false }
    }

    /// Checks if a nudge should be added.
    ///
    /// - `Some(count)`: Execution continues - check for interval nudges
    /// - `None`: Execution yielding - send completion check (once)
    pub fn should_nudge(&mut self, event_count: Option<usize>) -> bool {
        match event_count {
            Some(count) => self
                .interval
                .is_some_and(|n| count > 0 && count.is_multiple_of(n)),
            None => !std::mem::replace(&mut self.completion_nudge, true),
        }
    }

    pub fn reset_completion_nudge(&mut self) {
        self.completion_nudge = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_nudge_at_interval_boundary() {
        // At interval boundary (e.g., every 5 requests)
        let interval = Some(5);
        let mut fixture = Nudger::new(&interval);

        assert!(fixture.should_nudge(Some(5)));
        assert!(fixture.should_nudge(Some(10)));
    }

    #[test]
    fn test_should_not_nudge_between_boundaries() {
        // Between interval boundaries
        let interval = Some(5);
        let mut fixture = Nudger::new(&interval);

        assert!(!fixture.should_nudge(Some(3)));
        assert!(!fixture.should_nudge(Some(7)));
    }

    #[test]
    fn test_should_nudge_once_on_yield() {
        // On yield, should nudge once for completion check
        let interval = Some(5);
        let mut fixture = Nudger::new(&interval);

        assert!(fixture.should_nudge(None));
        assert!(!fixture.should_nudge(None)); // Second yield - no nudge
    }
}
