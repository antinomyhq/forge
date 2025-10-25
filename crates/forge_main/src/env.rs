use forge_api::{AgentId, ConversationId};

// Environment variable names
pub const FORGE_CONVERSATION_ID: &str = "FORGE_CONVERSATION_ID";
pub const FORGE_ACTIVE_AGENT: &str = "FORGE_ACTIVE_AGENT";
pub const FORGE_SHOW_TASK_STATS: &str = "FORGE_SHOW_TASK_STATS";

/// Get conversation ID from FORGE_CONVERSATION_ID environment variable
pub fn get_conversation_id_from_env() -> Option<ConversationId> {
    std::env::var(FORGE_CONVERSATION_ID)
        .ok()
        .and_then(|env_id| forge_domain::ConversationId::parse(&env_id).ok())
}

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
    fn test_get_conversation_id_from_env_with_valid_id() {
        let fixture_env_value = "01234567-89ab-cdef-0123-456789abcdef";
        unsafe {
            std::env::set_var(FORGE_CONVERSATION_ID, fixture_env_value);
        }

        let actual = get_conversation_id_from_env();
        let expected = forge_domain::ConversationId::parse(fixture_env_value).ok();

        assert_eq!(actual, expected);
        unsafe {
            std::env::remove_var(FORGE_CONVERSATION_ID);
        }
    }

    #[test]
    fn test_get_conversation_id_from_env_with_invalid_id() {
        let fixture_env_value = "invalid-uuid";
        unsafe {
            std::env::set_var(FORGE_CONVERSATION_ID, fixture_env_value);
        }

        let actual = get_conversation_id_from_env();
        let expected = None;

        assert_eq!(actual, expected);
        unsafe {
            std::env::remove_var(FORGE_CONVERSATION_ID);
        }
    }

    #[test]
    fn test_get_conversation_id_from_env_not_set() {
        unsafe {
            std::env::remove_var(FORGE_CONVERSATION_ID);
        }

        let actual = get_conversation_id_from_env();
        let expected = None;

        assert_eq!(actual, expected);
    }

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
