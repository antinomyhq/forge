/// Manages plan nudge decisions within the execution loop
pub struct PlanNudger<'a> {
    has_message: bool,
    interval: &'a Option<usize>,
    yield_nudge_used: bool,
}

impl<'a> PlanNudger<'a> {
    /// Creates a new plan nudger
    pub fn new(has_nudge_message: bool, interval: &'a Option<usize>) -> Self {
        Self {
            has_message: has_nudge_message,
            interval,
            yield_nudge_used: false,
        }
    }

    /// Determines if a nudge should be added based on yield decision and
    /// request count.
    pub(crate) fn should_add_nudge(&mut self, should_yield: bool, request_count: usize) -> bool {
        if should_yield {
            // Agent has decided to yeild control of agetic loop.
            // 1. if agent is executing plan then we've to nudge agent
            //    last time to check if agent has executed the plan
            //    completely or not.
            if (self.should_add_interval_nudge(request_count)
                || self.should_add_yield_nudge(request_count))
                && !self.yield_nudge_used
            {
                self.yield_nudge_used = true;
                return true;
            } else {
                self.yield_nudge_used = false;
                return false;
            }
        }

        self.yield_nudge_used = false;
        return self.should_add_interval_nudge(request_count);
    }

    /// Checks if interval-based nudge should be sent at current request count
    fn should_add_interval_nudge(&self, request_count: usize) -> bool {
        self.has_message
            && self
                .interval
                .is_some_and(|n| request_count > 0 && request_count.is_multiple_of(n))
    }

    /// Checks if yield nudge should be sent.
    /// Returns true when:
    /// - We have a nudge message configured
    /// - Yield nudge hasn't been used yet
    /// - Next iteration won't be an interval nudge (to avoid duplication)
    fn should_add_yield_nudge(&self, request_count: usize) -> bool {
        if !self.has_message || self.yield_nudge_used {
            return false;
        }
        // Don't add yield nudge if next iteration would be an interval nudge
        !self.should_add_interval_nudge(request_count)
    }
}
