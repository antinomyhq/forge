use std::fmt::Display;

use serde::{Deserialize, Serialize};

// Base URLs for providers.
const OPEN_ROUTER_URL: &str = "https://openrouter.ai/api/v1/";
const OPENAI_URL: &str = "https://api.openai.com/v1/";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/";

// Environment variables for providers.
const OPEN_AI: &str = "OPEN_AI_KEY";
const OPEN_ROUTER: &str = "OPEN_ROUTER_KEY";
const ANTHROPIC: &str = "ANTHROPIC_KEY";
const FORGE: &str = "FORGE_KEY";
const FORGE_PROVIDER_URL: &str = "FORGE_PROVIDER_URL";

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
    /// Maps environment variables to provider
    pub fn from_env() -> anyhow::Result<Self> {
        match (
            std::env::var(FORGE),
            std::env::var(OPEN_ROUTER),
            std::env::var(OPEN_AI),
            std::env::var(ANTHROPIC),
        ) {
            (Ok(_), _, _, _) => {
                // note: if we're using FORGE_KEY, we need FORGE_PROVIDER_URL to be set.
                let provider_url = std::env::var(FORGE_PROVIDER_URL).map_err(|_| anyhow::anyhow!("FORGE_PROVIDER_URL must be set for FORGE_KEY"))?;
                Self::from_url(&provider_url)
            }
            (_, Ok(_), _, _) => Ok(Self::OpenRouter),
            (_, _, Ok(_), _) => Ok(Self::OpenAI),
            (_, _, _, Ok(_)) => Ok(Self::Anthropic),
            (Err(_), Err(_), Err(_), Err(_)) => Err(anyhow::anyhow!("No provider key found, please set one of: FORGE_KEY, OPEN_ROUTER_KEY, OPEN_AI_KEY or ANTHROPIC_KEY")),
        }
    }

    /// Maps provider to it's base URL
    pub fn to_base_url(&self) -> &str {
        match self {
            Provider::OpenRouter => OPEN_ROUTER_URL,
            Provider::OpenAI => OPENAI_URL,
            Provider::Anthropic => ANTHROPIC_URL,
        }
    }

    /// Reads the key for provider from env.
    pub fn to_key(&self) -> Option<String> {
        // note: `forge` env can hold the key for all providers.
        match self {
            Provider::OpenRouter => std::env::var(OPEN_ROUTER).or(std::env::var(FORGE)),
            Provider::OpenAI => std::env::var(OPEN_AI).or(std::env::var(FORGE)),
            Provider::Anthropic => std::env::var(ANTHROPIC).or(std::env::var(FORGE)),
        }
        .ok()
    }

    /// Converts url to provider
    pub fn from_url(url: &str) -> anyhow::Result<Self> {
        match url {
            OPENAI_URL => Ok(Self::OpenAI),
            OPEN_ROUTER_URL => Ok(Self::OpenRouter),
            ANTHROPIC_URL => Ok(Self::Anthropic),
            _ => Err(anyhow::anyhow!("Provider for '{}' not found", url)),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::panic;
    use std::env;

    use derive_setters::Setters;

    use super::*;

    #[derive(Setters)]
    #[setters[into, strip_option]]
    struct EnvTest {
        title: Option<String>,
        env_var: Vec<(String, String)>, // key:value
        test: fn(),                     // test to execute under the given env
    }

    impl EnvTest {
        pub fn new(test: fn()) -> Self {
            Self { title: None, env_var: vec![], test }
        }

        pub fn add_env_var(mut self, key: &str, value: &str) -> Self {
            self.env_var.push((key.to_string(), value.to_string()));
            self
        }

        pub fn set_env(&self) {
            for (key, value) in &self.env_var {
                env::set_var(key, value);
            }
        }

        pub fn remove_env(&self) {
            for (key, _) in &self.env_var {
                env::remove_var(key);
            }
        }
    }

    #[derive(Default)]
    struct EnvTesterExecutor(Vec<EnvTest>);

    impl EnvTesterExecutor {
        pub fn add(&mut self, test: EnvTest) {
            self.0.push(test);
        }

        pub fn execute(self) {
            let mut failed_tests = vec![];
            for (idx, test) in self.0.into_iter().enumerate() {
                test.set_env();
                let result = std::panic::catch_unwind(|| (test.test)());
                if result.is_err() {
                    if let Some(ref title) = test.title {
                        failed_tests.push(format!("Test failed: {}", title));
                    } else {
                        failed_tests.push(format!("Test failed: Test No: {}", idx));
                    }
                }
                test.remove_env();
            }

            if !failed_tests.is_empty() {
                panic!("\n{}", failed_tests.join("\n"));
            }
        }
    }

    #[test]
    fn test_from_env() {
        let mut test_executor = EnvTesterExecutor::default();

        test_executor.add(
            EnvTest::new(|| {
                let provider = Provider::from_env();
                assert!(provider.is_err());
            })
            .add_env_var(FORGE, "")
            .title("test_provider_from_env_with_empty_forge_key"),
        );

        test_executor.add(
            EnvTest::new(|| {
                let provider = Provider::from_env();
                assert_eq!(*provider.as_ref().unwrap(), Provider::OpenAI);
                assert_eq!(
                    provider.unwrap().to_key(),
                    Some("some_forge_key".to_string())
                );
            })
            .title("test_provider_from_env_with_forge_key")
            .add_env_var(FORGE, "some_forge_key")
            .add_env_var(FORGE_PROVIDER_URL, "https://api.openai.com/v1/"),
        );

        test_executor.add(
            EnvTest::new(|| {
                let provider = Provider::from_env();
                assert_eq!(*provider.as_ref().unwrap(), Provider::OpenRouter);
                assert_eq!(
                    provider.unwrap().to_key(),
                    Some("some_open_router_key".to_string())
                );
            })
            .title("test_provider_from_env_with_open_router_key")
            .add_env_var(OPEN_ROUTER, "some_open_router_key"),
        );

        test_executor.add(
            EnvTest::new(|| {
                let provider = Provider::from_env();
                assert_eq!(*provider.as_ref().unwrap(), Provider::OpenAI);
                assert_eq!(
                    provider.unwrap().to_key(),
                    Some("some_openai_key".to_string())
                );
            })
            .add_env_var(OPEN_AI, "some_openai_key")
            .title("test_provider_from_env_with_openai_key"),
        );

        test_executor.add(
            EnvTest::new(|| {
                let provider = Provider::from_env();
                assert_eq!(*provider.as_ref().unwrap(), Provider::Anthropic);
                assert_eq!(
                    provider.unwrap().to_key(),
                    Some("some_anthropic_key".to_string())
                );
            })
            .title("test_provider_from_env_with_anthropic_key")
            .add_env_var(ANTHROPIC, "some_anthropic_key"),
        );

        test_executor.add(
            EnvTest::new(|| {
                let provider = Provider::from_env();
                assert!(provider.is_err());
            })
            .title("test_provider_from_env_with_no_keys"),
        );

        test_executor.execute(); // Run all tests
    }

    #[test]
    fn test_from_url() {
        assert_eq!(
            Provider::from_url("https://api.openai.com/v1/").unwrap(),
            Provider::OpenAI
        );
        assert_eq!(
            Provider::from_url("https://openrouter.ai/api/v1/").unwrap(),
            Provider::OpenRouter
        );
        assert_eq!(
            Provider::from_url("https://api.anthropic.com/v1/").unwrap(),
            Provider::Anthropic
        );
        assert!(Provider::from_url("https://unknown.url/").is_err());
    }

    #[test]
    fn test_to_url() {
        assert_eq!(Provider::OpenAI.to_base_url(), OPENAI_URL);
        assert_eq!(Provider::OpenRouter.to_base_url(), OPEN_ROUTER_URL);
        assert_eq!(Provider::Anthropic.to_base_url(), ANTHROPIC_URL);
    }
}
