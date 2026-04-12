use forge_app::EnvironmentInfra;
use forge_domain::{TerminalContext, TerminalCommand};

/// Service that reads terminal context from environment variables exported by
/// the zsh plugin and constructs a structured [`TerminalContext`].
///
/// The zsh plugin exports three colon-separated environment variables before
/// invoking forge:
/// - `FORGE_TERM_COMMANDS`   — the command strings
/// - `FORGE_TERM_EXIT_CODES` — the corresponding exit codes
/// - `FORGE_TERM_TIMESTAMPS` — the corresponding Unix timestamps
#[derive(Clone)]
pub struct TerminalContextService<S>(std::sync::Arc<S>);

impl<S> TerminalContextService<S> {
    /// Creates a new `TerminalContextService` backed by the provided infrastructure.
    pub fn new(infra: std::sync::Arc<S>) -> Self {
        Self(infra)
    }
}

impl<S: EnvironmentInfra> TerminalContextService<S> {
    /// Reads the terminal context from environment variables.
    ///
    /// Returns `None` if none of the required variables are set or if no
    /// commands were recorded.
    pub fn get_terminal_context(&self) -> Option<TerminalContext> {

        // FIXME: Move the env variable names to a const
        // FIXME: Add use `_FORGE_TERM_...` as the prefix
        let commands_raw = self.0.get_env_var("FORGE_TERM_COMMANDS")?;
        let exit_codes_raw = self
            .0
            .get_env_var("FORGE_TERM_EXIT_CODES")
            .unwrap_or_default();
        let timestamps_raw = self
            .0
            .get_env_var("FORGE_TERM_TIMESTAMPS")
            .unwrap_or_default();

        let commands: Vec<String> = split_env_list(&commands_raw);
        if commands.is_empty() {
            return None;
        }

        let exit_codes: Vec<i32> = split_env_list(&exit_codes_raw)
            .iter()
            .map(|s| s.parse::<i32>().unwrap_or(0))
            .collect();

        let timestamps: Vec<u64> = split_env_list(&timestamps_raw)
            .iter()
            .map(|s| s.parse::<u64>().unwrap_or(0))
            .collect();

        // Zip the three lists together, stopping at the shortest
        let entries: Vec<TerminalCommand> = commands
            .into_iter()
            .zip(
                exit_codes
                    .into_iter()
                    .chain(std::iter::repeat(0))
                    .take(usize::MAX),
            )
            .zip(
                timestamps
                    .into_iter()
                    .chain(std::iter::repeat(0))
                    .take(usize::MAX),
            )
            .map(|((command, exit_code), timestamp)| TerminalCommand {
                command,
                exit_code,
                timestamp,
            })
            .collect();

        if entries.is_empty() {
            None
        } else {
            Some(TerminalContext { commands: entries })
        }
    }
}

/// Splits a colon-separated environment variable value into a list of strings,
/// filtering out any empty segments produced by leading/trailing/double colons.
fn split_env_list(raw: &str) -> Vec<String> {
    raw.split(':')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use forge_domain::{Environment, TerminalCommand, TerminalContext};
    use pretty_assertions::assert_eq;

    use super::*;

    struct MockInfra {
        env_vars: BTreeMap<String, String>,
    }

    impl MockInfra {
        fn new(vars: &[(&str, &str)]) -> Arc<Self> {
            Arc::new(Self {
                env_vars: vars
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            })
        }
    }

    impl forge_app::EnvironmentInfra for MockInfra {
        type Config = forge_config::ForgeConfig;

        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }

        fn get_config(&self) -> anyhow::Result<forge_config::ForgeConfig> {
            Ok(forge_config::ForgeConfig::default())
        }

        async fn update_environment(
            &self,
            _ops: Vec<forge_domain::ConfigOperation>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_env_var(&self, key: &str) -> Option<String> {
            self.env_vars.get(key).cloned()
        }

        fn get_env_vars(&self) -> BTreeMap<String, String> {
            self.env_vars.clone()
        }
    }

    #[test]
    fn test_no_env_vars_returns_none() {
        let fixture = TerminalContextService::new(MockInfra::new(&[]));
        let actual = fixture.get_terminal_context();
        assert_eq!(actual, None);
    }

    #[test]
    fn test_empty_commands_returns_none() {
        let fixture = TerminalContextService::new(MockInfra::new(&[
            ("FORGE_TERM_COMMANDS", ""),
        ]));
        let actual = fixture.get_terminal_context();
        assert_eq!(actual, None);
    }

    #[test]
    fn test_single_command_no_extras() {
        let fixture = TerminalContextService::new(MockInfra::new(&[
            ("FORGE_TERM_COMMANDS", "cargo build"),
        ]));
        let actual = fixture.get_terminal_context();
        let expected = Some(TerminalContext {
            commands: vec![TerminalCommand {
                command: "cargo build".to_string(),
                exit_code: 0,
                timestamp: 0,
            }],
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_multiple_commands_with_exit_codes_and_timestamps() {
        let fixture = TerminalContextService::new(MockInfra::new(&[
            ("FORGE_TERM_COMMANDS", "ls:cargo test:git status"),
            ("FORGE_TERM_EXIT_CODES", "0:1:0"),
            ("FORGE_TERM_TIMESTAMPS", "1700000001:1700000002:1700000003"),
        ]));
        let actual = fixture.get_terminal_context();
        let expected = Some(TerminalContext {
            commands: vec![
                TerminalCommand {
                    command: "ls".to_string(),
                    exit_code: 0,
                    timestamp: 1700000001,
                },
                TerminalCommand {
                    command: "cargo test".to_string(),
                    exit_code: 1,
                    timestamp: 1700000002,
                },
                TerminalCommand {
                    command: "git status".to_string(),
                    exit_code: 0,
                    timestamp: 1700000003,
                },
            ],
        });
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_split_env_list_empty() {
        let actual = split_env_list("");
        let expected: Vec<String> = vec![];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_split_env_list_single() {
        let actual = split_env_list("hello");
        let expected = vec!["hello".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_split_env_list_multiple() {
        let actual = split_env_list("a:b:c");
        let expected = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(actual, expected);
    }
}
