use forge_api::{AgentId, ConversationId};

// Environment variable names
pub const FORGE_CONVERSATION_ID: &str = "FORGE_CONVERSATION_ID";
pub const FORGE_ACTIVE_AGENT: &str = "FORGE_ACTIVE_AGENT";

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

/// Parses environment variable strings in KEY=VALUE format into a BTreeMap
///
/// Takes a vector of strings where each string should be in the format
/// "KEY=VALUE" and returns a BTreeMap with the parsed key-value pairs. Invalid
/// entries (without an '=' separator) are silently skipped.
pub fn parse_env(env: Vec<String>) -> std::collections::BTreeMap<String, String> {
    env.into_iter()
        .filter_map(|s| {
            let mut parts = s.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                Some((key.to_string(), value.to_string()))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

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
    fn test_get_agent_from_env_not_set() {
        unsafe {
            std::env::remove_var(FORGE_ACTIVE_AGENT);
        }

        let actual = get_agent_from_env();
        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_env_with_valid_entries() {
        let fixture = vec![
            "HOME=/home/user".to_string(),
            "PATH=/usr/bin".to_string(),
            "LANG=en_US.UTF-8".to_string(),
        ];

        let actual = parse_env(fixture);

        assert_eq!(actual.get("HOME"), Some(&"/home/user".to_string()));
        assert_eq!(actual.get("PATH"), Some(&"/usr/bin".to_string()));
        assert_eq!(actual.get("LANG"), Some(&"en_US.UTF-8".to_string()));
        assert_eq!(actual.len(), 3);
    }

    #[test]
    fn test_parse_env_with_empty_vector() {
        let fixture = vec![];

        let actual = parse_env(fixture);
        let expected = std::collections::BTreeMap::new();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_env_with_invalid_entries() {
        let fixture = vec![
            "VALID=value".to_string(),
            "INVALID_NO_EQUALS".to_string(),
            "ANOTHER=valid".to_string(),
        ];

        let actual = parse_env(fixture);

        assert_eq!(actual.get("VALID"), Some(&"value".to_string()));
        assert_eq!(actual.get("ANOTHER"), Some(&"valid".to_string()));
        assert_eq!(actual.get("INVALID_NO_EQUALS"), None);
        assert_eq!(actual.len(), 2);
    }

    #[test]
    fn test_parse_env_with_equals_in_value() {
        let fixture = vec!["KEY=value=with=equals".to_string()];

        let actual = parse_env(fixture);

        assert_eq!(actual.get("KEY"), Some(&"value=with=equals".to_string()));
    }
}
