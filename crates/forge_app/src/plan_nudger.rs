/// Manages plan nudge decisions within the execution loop
pub(crate) struct PlanNudger<'a> {
    has_message: bool,
    interval: &'a Option<usize>,
    yield_nudge_used: bool,
}

impl<'a> PlanNudger<'a> {
    /// Creates a new plan nudger
    pub(crate) fn new(has_nudge_message: bool, interval: &'a Option<usize>) -> Self {
        Self {
            has_message: has_nudge_message,
            interval,
            yield_nudge_used: false,
        }
    }

    /// Checks if interval-based nudge should be sent at current request count
    pub(crate) fn should_add_interval_nudge(&self, request_count: &usize) -> bool {
        self.has_message
            && self
                .interval
                .is_some_and(|n| *request_count > 0 && request_count.is_multiple_of(n))
    }

    /// Checks if yield nudge should be sent.
    /// Returns true when:
    /// - We have a nudge message configured
    /// - We have an interval configured (nudging is enabled)
    /// - Yield nudge hasn't been used yet
    /// - Next iteration won't be an interval nudge (to avoid duplication)
    pub(crate) fn should_add_yield_nudge(&self, request_count: &usize) -> bool {
        if !self.has_message || self.yield_nudge_used {
            return false;
        }

        // Don't add yield nudge if next iteration would be an interval nudge
        !self.should_add_interval_nudge(&request_count)
    }

    /// Marks that the yield nudge has been sent (consumes the one-time nudge)
    pub(crate) fn mark_yield_nudge(&mut self) {
        self.yield_nudge_used = true;
    }

    /// Resets the yield nudge flag when not yielding
    pub(crate) fn reset_yeild_nudge(&mut self) {
        self.yield_nudge_used = false;
    }
}
