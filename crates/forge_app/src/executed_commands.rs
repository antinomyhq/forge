use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_domain::Environment;

use crate::{EnvironmentInfra, FileReaderInfra};

const ZSH_HISTORY_FILE: &str = ".zsh_history";
const BASH_HISTORY_FILE: &str = ".bash_history";

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
    Unknown,
}

impl HistoryFile {
    /// Determines the history file type based on the file name
    fn from_path(path: &Path) -> Self {
        match path.file_name().and_then(|n| n.to_str()) {
            Some(ZSH_HISTORY_FILE) => Self::Zsh,
            Some(BASH_HISTORY_FILE) => Self::Bash,
            _ => Self::Unknown,
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
        }
    }

    /// Resolves the history file path for a given shell
    fn resolve_for_shell(home: &Path, shell: &str) -> Option<PathBuf> {
        let filename = if shell.contains("zsh") {
            ZSH_HISTORY_FILE
        } else if shell.contains("bash") {
            BASH_HISTORY_FILE
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

        let filtered_commands: Vec<_> = file_type
            .parse(&content)
            .into_iter()
            .filter_map(|cmd| {
                let cmd = cmd.trim();
                if cmd.is_empty() || cmd.starts_with("forge") || cmd.starts_with(':') {
                    None
                } else {
                    Some(cmd.to_owned())
                }
            })
            .collect();

        // Take the last N commands in chronological order
        let start = filtered_commands.len().saturating_sub(limit);
        Ok(filtered_commands[start..].to_vec())
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
        let fixture_unknown = PathBuf::from("/home/user/.fish_history");

        let actual_zsh = HistoryFile::from_path(&fixture_zsh);
        let actual_bash = HistoryFile::from_path(&fixture_bash);
        let actual_unknown = HistoryFile::from_path(&fixture_unknown);

        assert!(matches!(actual_zsh, HistoryFile::Zsh));
        assert!(matches!(actual_bash, HistoryFile::Bash));
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
}
