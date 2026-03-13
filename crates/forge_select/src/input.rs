use std::io::{self, Write};
use std::process::{Command, Stdio};

use anyhow::Result;
use colored::Colorize;

/// Escapes a string for safe embedding as a shell single-quoted argument.
///
/// Single-quotes in the input are replaced with `\'\''` (end quote, literal
/// single-quote, reopen quote) so the entire result can be wrapped in `'...'`.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Builds the shell script used by input prompts.
///
/// Uses Bash `read -e` so Readline handles cursor movement keys such as
/// left/right arrows while still reading directly from `/dev/tty`.
fn build_input_prompt_script(prompt: &str) -> String {
    format!(
        "printf '%s' {prompt} >&2; read -e -r FORGE_INPUT </dev/tty && printf '%s' \"$FORGE_INPUT\"",
        prompt = shell_escape(prompt),
    )
}

/// Strips bracketed-paste escape sequences from a string.
///
/// When bracketed paste mode is active in the terminal, pasted text is wrapped
/// in `\x1b[200~` (start) and `\x1b[201~` (end) markers. This function removes
/// those markers from the captured shell output so the raw input value is
/// clean.
fn strip_bracketed_paste(s: &str) -> String {
    s.replace("\x1b[200~", "").replace("\x1b[201~", "")
}

/// Builder for input prompts.
pub struct InputBuilder {
    pub(crate) message: String,
    pub(crate) allow_empty: bool,
    pub(crate) default: Option<String>,
    pub(crate) default_display: Option<String>,
}

impl InputBuilder {
    /// Allow empty input.
    pub fn allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    /// Set default value.
    pub fn with_default<T>(mut self, default: T) -> Self
    where
        T: std::fmt::Display + AsRef<str>,
    {
        self.default = Some(default.as_ref().to_string());
        self.default_display = Some(default.to_string());
        self
    }

    /// Execute input prompt using a shell-native `read` command.
    ///
    /// Delegates to `bash -c 'read -e -r VAR ...'` via `/dev/tty` so Readline
    /// handles cursor movement keys and terminal state issues caused by prior
    /// fzf invocations (raw mode, SIGCHLD, etc.) do not affect input reading.
    /// When `allow_empty` is false and no default is set, re-prompts until
    /// non-empty input is provided.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(String))` - User provided input
    /// - `Ok(None)` - User cancelled (Ctrl+C / EOF / shell error)
    ///
    /// # Errors
    ///
    /// Returns an error if spawning the shell subprocess fails.
    pub fn prompt(self) -> Result<Option<String>> {
        let hint = match (&self.default, &self.default_display) {
            (Some(val), Some(display)) if val != display => Some(display.clone()),
            (Some(val), _) => Some(val.clone()),
            _ => None,
        };

        loop {
            let prompt_str = match &hint {
                Some(h) => format!(
                    "{} {} {}: ",
                    "?".yellow().bold(),
                    self.message.bold(),
                    format!("({})", h).dimmed(),
                ),
                None => format!("{} {}: ", "?".yellow().bold(), self.message.bold()),
            };

            let script = build_input_prompt_script(&prompt_str);

            let output = Command::new("bash")
                .arg("-c")
                .arg(&script)
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to spawn shell for input: {e}"))?;

            if !output.status.success() {
                return Ok(None);
            }

            let raw = String::from_utf8_lossy(&output.stdout).to_string();
            let value = strip_bracketed_paste(&raw);
            let trimmed = value.trim();

            if trimmed.is_empty() {
                if let Some(ref default_val) = self.default {
                    return Ok(Some(default_val.clone()));
                }
                if self.allow_empty {
                    return Ok(Some(String::new()));
                }
                let mut out = io::stdout();
                writeln!(out, "Input cannot be empty. Please try again.")?;
                continue;
            }

            return Ok(Some(trimmed.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::ForgeWidget;

    #[test]
    fn test_input_builder_creates() {
        let builder = ForgeWidget::input("Enter name:");
        assert_eq!(builder.message, "Enter name:");
        assert_eq!(builder.allow_empty, false);
    }

    #[test]
    fn test_input_builder_with_default() {
        let builder = ForgeWidget::input("Enter key:").with_default("mykey");
        assert_eq!(builder.default, Some("mykey".to_string()));
    }

    #[test]
    fn test_input_builder_allow_empty() {
        let builder = ForgeWidget::input("Enter:").allow_empty(true);
        assert_eq!(builder.allow_empty, true);
    }

    #[test]
    fn test_build_input_prompt_script_uses_bash_readline() {
        let fixture = "? Enter key: ";
        let actual = build_input_prompt_script(fixture);
        let expected =
            "printf '%s' '? Enter key: ' >&2; read -e -r FORGE_INPUT </dev/tty && printf '%s' \"$FORGE_INPUT\""
                .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_build_input_prompt_script_escapes_single_quotes() {
        let fixture = "? Enter John's key: ";
        let actual = build_input_prompt_script(fixture);
        let expected = r##"printf '%s' '? Enter John'\''s key: ' >&2; read -e -r FORGE_INPUT </dev/tty && printf '%s' "$FORGE_INPUT""##
            .to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_strip_bracketed_paste() {
        let fixture = "\x1b[200~myapikey\x1b[201~";
        let actual = strip_bracketed_paste(fixture);
        let expected = "myapikey";
        assert_eq!(actual, expected);

        let fixture = "myapikey";
        let actual = strip_bracketed_paste(fixture);
        let expected = "myapikey";
        assert_eq!(actual, expected);

        let fixture = "\x1b[200~myapikey";
        let actual = strip_bracketed_paste(fixture);
        let expected = "myapikey";
        assert_eq!(actual, expected);
    }
}
