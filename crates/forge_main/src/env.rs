use forge_api::AgentId;

// Environment variable names
pub const FORGE_ACTIVE_AGENT: &str = "FORGE_ACTIVE_AGENT";
pub const FORGE_SHOW_TASK_STATS: &str = "FORGE_SHOW_TASK_STATS";

/// Get agent ID from FORGE_ACTIVE_AGENT environment variable
pub fn get_agent_from_env() -> Option<AgentId> {
    std::env::var(FORGE_ACTIVE_AGENT).ok().map(AgentId::new)
}

/// Check if the completion prompt should be shown
///
/// Returns true if the environment variable is not set, cannot be parsed, or is
/// set to "true" (case-insensitive). Returns false only if explicitly set to
/// "false".
pub fn should_show_completion_prompt() -> bool {
    std::env::var(FORGE_SHOW_TASK_STATS)
        .ok()
        .and_then(|val| val.trim().parse::<bool>().ok())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serial_test::serial;

    use super::*;

    #[test]
    #[serial]
    fn test_get_agent_from_env_with_value() {
        let fixture_env_value = "sage";
        unsafe {
            std::env::set_var(FORGE_ACTIVE_AGENT, fixture_env_value);
        }

        let actual = get_agent_from_env();
        let expected = Some(AgentId::new("sage"));

        assert_eq!(actual, expected);
        unsafe {
            std::env::remove_var(FORGE_ACTIVE_AGENT);
        }
    }

    #[test]
    #[serial]
    fn test_get_agent_from_env_not_set() {
        unsafe {
            std::env::remove_var(FORGE_ACTIVE_AGENT);
        }

        let actual = get_agent_from_env();
        let expected = None;

        assert_eq!(actual, expected);
    }
}
