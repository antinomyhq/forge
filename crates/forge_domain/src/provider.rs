use std::fmt::Display;

use serde::{Deserialize, Serialize};

const OPEN_ROUTER_URL: &str = "https://api.openrouter.io/v1/";
const OPENAI_URL: &str = "https://api.openai.com/v1/";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/";

/// Providers that can be used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Provider {
    OpenRouter,
    OpenAI,
    Anthropic,
}

impl Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::OpenRouter => write!(f, "OpenRouter"),
            Provider::OpenAI => write!(f, "OpenAI"),
            Provider::Anthropic => write!(f, "Anthropic"),
        }
    }
}

impl Provider {
    // detects the active provider from environment variables
    pub fn from_env() -> Option<Self> {
        match (
            std::env::var("FORGE_KEY"),
            std::env::var("OPEN_ROUTER_KEY"),
            std::env::var("OPENAI_API_KEY"),
            std::env::var("ANTHROPIC_API_KEY"),
        ) {
            (Ok(_), _, _, _) => {
                // note: if we're using FORGE_KEY, we need FORGE_PROVIDER_URL to be set.
                let provider_url = std::env::var("FORGE_PROVIDER_URL").ok()?;
                Self::from_url(&provider_url)
            }
            (_, Ok(_), _, _) => Some(Self::OpenRouter),
            (_, _, Ok(_), _) => Some(Self::OpenAI),
            (_, _, _, Ok(_)) => Some(Self::Anthropic),
            (Err(_), Err(_), Err(_), Err(_)) => None,
        }
    }

    /// converts the provider to it's base URL
    pub fn to_base_url(&self) -> &str {
        match self {
            Provider::OpenRouter => OPEN_ROUTER_URL,
            Provider::OpenAI => OPENAI_URL,
            Provider::Anthropic => ANTHROPIC_URL,
        }
    }

    /// detects the active provider from base URL
    pub fn from_url(url: &str) -> Option<Self> {
        match url {
            OPENAI_URL => Some(Self::OpenAI),
            OPEN_ROUTER_URL => Some(Self::OpenRouter),
            ANTHROPIC_URL => Some(Self::Anthropic),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::sync::Mutex;

    use lazy_static::lazy_static;

    use super::*;

    // reset the env variables for reliable tests
    lazy_static! {
        static ref ENV_LOCK: Mutex<()> = Mutex::new(());
    }

    fn with_env(envs: Vec<(String, String)>, env_based_test: impl FnOnce()) {
        // Create a new guard without unwrapping - if it fails, we'll just run without the lock
        // This will allow tests to continue even if another test already has the lock
        let _guard = ENV_LOCK.lock();
        
        // Clear existing environment variables that might interfere with our test
        env::remove_var("FORGE_KEY");
        env::remove_var("FORGE_PROVIDER_URL");
        env::remove_var("OPEN_ROUTER_KEY");
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("ANTHROPIC_API_KEY");

        // Set the environment variables for this test
        for (key, value) in envs.iter() {
            env::set_var(key, value);
        }

        // Run the actual test
        env_based_test();

        // Clean up
        for (key, _) in envs.iter() {
            env::remove_var(key);
        }
    }

    #[test]
    fn test_provider_from_env_with_forge_key_and_without_provider_url() {
        with_env(
            vec![("FORGE_KEY".to_string(), "some_forge_key".to_string())],
            || {
                let provider = Provider::from_env();
                assert_eq!(provider, None);
            },
        );
    }

    #[test]
    fn test_provider_from_env_with_forge_key() {
        with_env(
            vec![
                ("FORGE_KEY".to_string(), "some_forge_key".to_string()),
                (
                    "FORGE_PROVIDER_URL".to_string(),
                    "https://api.openai.com/v1/".to_string(),
                ),
            ],
            || {
                let provider = Provider::from_env();
                assert_eq!(provider, Some(Provider::OpenAI));
            },
        );
    }

    #[test]
    fn test_provider_from_env_with_open_router_key() {
        with_env(
            vec![(
                "OPEN_ROUTER_KEY".to_string(),
                "some_open_router_key".to_string(),
            )],
            || {
                let provider = Provider::from_env();
                assert_eq!(provider, Some(Provider::OpenRouter));
            },
        );
    }

    #[test]
    fn test_provider_from_env_with_openai_key() {
        with_env(
            vec![("OPENAI_API_KEY".to_string(), "some_openai_key".to_string())],
            || {
                let provider = Provider::from_env();
                assert_eq!(provider, Some(Provider::OpenAI));
            },
        );
    }

    #[test]
    fn test_provider_from_env_with_anthropic_key() {
        with_env(
            vec![(
                "ANTHROPIC_API_KEY".to_string(),
                "some_anthropic_key".to_string(),
            )],
            || {
                let provider = Provider::from_env();
                assert_eq!(provider, Some(Provider::Anthropic));
            },
        );
    }

    #[test]
    fn test_provider_from_env_with_no_keys() {
        with_env(vec![], || {
            let provider = Provider::from_env();
            assert_eq!(provider, None);
        });
    }

    #[test]
    fn test_from_url() {
        assert_eq!(
            Provider::from_url("https://api.openai.com/v1/"),
            Some(Provider::OpenAI)
        );
        assert_eq!(
            Provider::from_url("https://api.openrouter.io/v1/"),
            Some(Provider::OpenRouter)
        );
        assert_eq!(
            Provider::from_url("https://api.anthropic.com/v1/"),
            Some(Provider::Anthropic)
        );
        assert_eq!(Provider::from_url("https://unknown.url/"), None);
    }

    #[test]
    fn test_to_url() {
        assert_eq!(Provider::OpenAI.to_base_url(), "https://api.openai.com/v1/");
        assert_eq!(
            Provider::OpenRouter.to_base_url(),
            "https://api.openrouter.io/v1/"
        );
        assert_eq!(
            Provider::Anthropic.to_base_url(),
            "https://api.anthropic.com/v1/"
        );
    }
}
