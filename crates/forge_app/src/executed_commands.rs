use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_domain::Environment;
use serde::Deserialize;

use crate::{EnvironmentInfra, FileReaderInfra};

const ZSH_HISTORY_FILE: &str = ".zsh_history";
const BASH_HISTORY_FILE: &str = ".bash_history";
const FISH_HISTORY_FILE: &str = ".local/share/fish/fish_history";

/// Represents a Fish shell history entry
#[derive(Debug, Deserialize)]
struct FishHistoryEntry {
    cmd: String,
}

/// Service for retrieving shell command history
pub struct ExecutedCommands<F> {
    infra: Arc<F>,
}

impl<F> ExecutedCommands<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

enum HistoryFile {
    Zsh,
    Bash,
    Fish,
    Unknown,
}

impl HistoryFile {
    /// Determines the history file type based on the file name
    fn from_path(path: &Path) -> Self {
        let path_str = path.to_string_lossy();
        if path_str.ends_with(ZSH_HISTORY_FILE) {
            Self::Zsh
        } else if path_str.ends_with(BASH_HISTORY_FILE) {
            Self::Bash
        } else if path_str.ends_with(FISH_HISTORY_FILE) {
            Self::Fish
        } else {
            Self::Unknown
        }
    }

    /// Parses history file content based on the shell format
    fn parse(self, content: &str) -> Vec<String> {
        match self {
            Self::Unknown | Self::Bash => content.lines().map(String::from).collect(),
            Self::Zsh => content
                .lines()
                .filter_map(|line| {
                    // Zsh history format: `: timestamp:duration;command`
                    line.split_once(';').map(|(_, cmd)| cmd.trim().to_string())
                })
                .collect(),
            Self::Fish => {
                // Fish history is in YAML format
                serde_yml::from_str::<Vec<FishHistoryEntry>>(content)
                    .map(|entries| entries.into_iter().map(|e| e.cmd).collect())
                    .unwrap_or_default()
            }
        }
    }

    /// Resolves the history file path for a given shell
    fn resolve_for_shell(home: &Path, shell: &str) -> Option<PathBuf> {
        let filename = if shell.contains("zsh") {
            ZSH_HISTORY_FILE
        } else if shell.contains("bash") {
            BASH_HISTORY_FILE
        } else if shell.contains("fish") {
            FISH_HISTORY_FILE
        } else {
            return None;
        };

        let path = home.join(filename);
        path.exists().then_some(path)
    }
}

impl<F: EnvironmentInfra + FileReaderInfra> ExecutedCommands<F> {
    /// Retrieves the most recent shell commands from history, excluding forge
    /// commands
    pub async fn shell_commands(
        &self,
        env: &Environment,
        limit: usize,
    ) -> anyhow::Result<Vec<String>> {
        let history_path = self
            .infra
            .get_env_var("HISTFILE")
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .or_else(|| {
                env.home
                    .as_deref()
                    .and_then(|home| HistoryFile::resolve_for_shell(home, &env.shell))
            });

        let Some(history_path) = history_path else {
            return Ok(Vec::new());
        };

        let content = self.infra.read_utf8(&history_path).await?;
        let file_type = HistoryFile::from_path(&history_path);

        let commands: Vec<_> = file_type
            .parse(&content)
            .into_iter()
            .filter_map(Self::filter_command)
            .collect();

        // Deduplicate commands while preserving order (keep last occurrence)
        let mut seen = std::collections::HashSet::new();
        let deduped: Vec<_> = commands
            .into_iter()
            .rev()
            .filter(|cmd| seen.insert(cmd.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        // Take the last N commands in chronological order
        let start = deduped.len().saturating_sub(limit);
        Ok(deduped[start..].to_vec())
    }

    fn filter_command(cmd: String) -> Option<String> {
        let trimmed = cmd.trim();
        if trimmed.is_empty() || trimmed.starts_with("forge") || trimmed.starts_with(':') {
            None
        } else {
            Some(trimmed.to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_history_file_from_path() {
        let fixture_zsh = PathBuf::from("/home/user/.zsh_history");
        let fixture_bash = PathBuf::from("/home/user/.bash_history");
        let fixture_fish = PathBuf::from("/home/user/.local/share/fish/fish_history");
        let fixture_unknown = PathBuf::from("/home/user/.unknown_history");

        let actual_zsh = HistoryFile::from_path(&fixture_zsh);
        let actual_bash = HistoryFile::from_path(&fixture_bash);
        let actual_fish = HistoryFile::from_path(&fixture_fish);
        let actual_unknown = HistoryFile::from_path(&fixture_unknown);

        assert!(matches!(actual_zsh, HistoryFile::Zsh));
        assert!(matches!(actual_bash, HistoryFile::Bash));
        assert!(matches!(actual_fish, HistoryFile::Fish));
        assert!(matches!(actual_unknown, HistoryFile::Unknown));
    }

    #[test]
    fn test_history_file_parse_bash() {
        let fixture = "echo hello\nls -la\ncd /tmp";
        let actual = HistoryFile::Bash.parse(fixture);

        let expected = vec![
            "echo hello".to_string(),
            "ls -la".to_string(),
            "cd /tmp".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_file_parse_unknown() {
        let fixture = "echo hello\nls -la\ncd /tmp";
        let actual = HistoryFile::Unknown.parse(fixture);

        let expected = vec![
            "echo hello".to_string(),
            "ls -la".to_string(),
            "cd /tmp".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_file_parse_zsh_format() {
        let fixture = ": 1234567890:0;echo hello\n: 1234567891:5;ls -la\n: 1234567892:10;cd /tmp";
        let actual = HistoryFile::Zsh.parse(fixture);
        let expected = vec![
            "echo hello".to_string(),
            "ls -la".to_string(),
            "cd /tmp".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_file_parse_zsh_skips_malformed_lines() {
        let fixture = ": 1234567890:0;echo hello\nmalformed line\n: 1234567891:5;ls -la";
        let actual = HistoryFile::Zsh.parse(fixture);
        let expected = vec!["echo hello".to_string(), "ls -la".to_string()];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_file_parse_fish_format() {
        let fixture = "- cmd: echo hello\n  when: 1234567890\n- cmd: ls -la\n  when: 1234567891\n- cmd: cd /tmp\n  when: 1234567892";
        let actual = HistoryFile::Fish.parse(fixture);
        let expected = vec![
            "echo hello".to_string(),
            "ls -la".to_string(),
            "cd /tmp".to_string(),
        ];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_file_parse_fish_handles_invalid_yaml() {
        let fixture = "this is not valid yaml";
        let actual = HistoryFile::Fish.parse(fixture);
        let expected: Vec<String> = vec![];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_history_file_parse_fish_with_metadata() {
        let fixture = "- cmd: echo hello\n  when: 1234567890\n- cmd: ls -la\n  when: 1234567891";
        let actual = HistoryFile::Fish.parse(fixture);
        let expected = vec!["echo hello".to_string(), "ls -la".to_string()];
        assert_eq!(actual, expected);
    }

    struct MockInfra {
        content: String,
        histfile: PathBuf,
    }

    impl MockInfra {
        pub fn new(content: String) -> Self {
            Self {
                content,
                histfile: PathBuf::from(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/executed_commands.rs"
                )),
            }
        }
    }

    impl crate::EnvironmentInfra for MockInfra {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }
        fn get_env_var(&self, key: &str) -> Option<String> {
            (key == "HISTFILE").then(|| self.histfile.to_string_lossy().to_string())
        }
    }

    #[async_trait::async_trait]
    impl crate::FileReaderInfra for MockInfra {
        async fn read_utf8(&self, _path: &Path) -> anyhow::Result<String> {
            Ok(self.content.clone())
        }
        async fn read(&self, _path: &Path) -> anyhow::Result<Vec<u8>> {
            unimplemented!()
        }
        async fn range_read_utf8(
            &self,
            _path: &Path,
            _start_line: u64,
            _end_line: u64,
        ) -> anyhow::Result<(String, forge_domain::FileInfo)> {
            unimplemented!()
        }
    }

    async fn get_recently_executed_shell_commands(content: &str, limit: usize) -> Vec<String> {
        use fake::{Fake, Faker};
        let fixture = ExecutedCommands::new(Arc::new(MockInfra::new(content.to_string())));
        let env: Environment = Faker.fake();
        fixture.shell_commands(&env, limit).await.unwrap()
    }

    #[tokio::test]
    async fn test_shell_commands_filters_forge_commands() {
        let actual =
            get_recently_executed_shell_commands("echo hello\nforge --help\ngit status", 10).await;
        let expected = vec!["echo hello".to_string(), "git status".to_string()];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_shell_commands_respects_limit() {
        let actual = get_recently_executed_shell_commands("cmd1\ncmd2\ncmd3\ncmd4\ncmd5", 3).await;
        let expected = vec!["cmd3".to_string(), "cmd4".to_string(), "cmd5".to_string()];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_shell_commands_deduplicates_keeping_last_occurrence() {
        let actual = get_recently_executed_shell_commands(
            "git status\nls\ngit status\necho hello\nls\npwd",
            10,
        )
        .await;
        let expected = vec![
            "git status".to_string(),
            "echo hello".to_string(),
            "ls".to_string(),
            "pwd".to_string(),
        ];
        assert_eq!(actual, expected);
    }
}
