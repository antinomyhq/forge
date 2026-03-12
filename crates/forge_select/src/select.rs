use std::io::{self, Write};
use std::process::{Command, Stdio};

use anyhow::Result;
use console::strip_ansi_codes;
use fzf_wrapped::{Fzf, Layout, run_with_output};

/// Centralized fzf-based select functionality with consistent error handling.
///
/// All interactive selection is delegated to the external `fzf` binary.
/// Requires `fzf` to be installed on the system.
pub struct ForgeSelect;

/// Builder for select prompts with fuzzy search.
pub struct SelectBuilder<T> {
    message: String,
    options: Vec<T>,
    default: Option<bool>,
    help_message: Option<&'static str>,
    initial_text: Option<String>,
}

/// Builder for select prompts that takes ownership (doesn't require Clone).
pub struct SelectBuilderOwned<T> {
    message: String,
    options: Vec<T>,
    initial_text: Option<String>,
}

impl ForgeSelect {
    /// Entry point for select operations with fuzzy search.
    pub fn select<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilder<T> {
        SelectBuilder {
            message: message.into(),
            options,
            default: None,
            help_message: None,
            initial_text: None,
        }
    }

    /// Entry point for select operations with owned values (doesn't require Clone).
    pub fn select_owned<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilderOwned<T> {
        SelectBuilderOwned {
            message: message.into(),
            options,
            initial_text: None,
        }
    }

    /// Convenience method for confirm (yes/no).
    pub fn confirm(message: impl Into<String>) -> SelectBuilder<bool> {
        SelectBuilder {
            message: message.into(),
            options: vec![true, false],
            default: None,
            help_message: None,
            initial_text: None,
        }
    }

    /// Prompt a question and get text input.
    pub fn input(message: impl Into<String>) -> InputBuilder {
        InputBuilder {
            message: message.into(),
            allow_empty: false,
            default: None,
            default_display: None,
        }
    }

    /// Multi-select prompt.
    pub fn multi_select<T>(message: impl Into<String>, options: Vec<T>) -> MultiSelectBuilder<T> {
        MultiSelectBuilder { message: message.into(), options }
    }
}

/// Builds an `Fzf` instance with standard layout and an optional header.
///
/// `--height=40%` is always added so fzf runs inline (below the current cursor)
/// rather than switching to the alternate screen buffer. Without this flag fzf
/// uses full-screen mode which enters the alternate screen (`\033[?1049h`),
/// making it appear as though the terminal is cleared.
fn build_fzf(header: &str, help_message: Option<&str>, initial_text: Option<&str>) -> Fzf {
    let mut builder = Fzf::builder();
    builder.layout(Layout::Reverse);

    let full_header = match help_message {
        Some(help) => format!("{}\n{}", header, help),
        None => header.to_string(),
    };
    builder.header(full_header);
    builder.header_first(true);

    // Combine all custom args in a single call — custom_args() replaces (not appends).
    let mut args = vec!["--height=40%".to_string()];
    if let Some(query) = initial_text {
        args.push(format!("--query={}", query));
    }
    builder.custom_args(args);

    builder.build().expect("fzf builder should always succeed with default options")
}

impl<T: 'static> SelectBuilder<T> {
    /// Set starting cursor position.
    ///
    /// Note: This is a no-op with fzf backend. fzf does not support pre-selecting
    /// a specific item by index. Users can use fuzzy search to quickly find items.
    pub fn with_starting_cursor(self, _cursor: usize) -> Self {
        self
    }

    /// Set default for confirm (only works with bool options).
    pub fn with_default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    /// Set help message displayed as a header above the list.
    pub fn with_help_message(mut self, message: &'static str) -> Self {
        self.help_message = Some(message);
        self
    }

    /// Set initial search text for fuzzy search.
    pub fn with_initial_text(mut self, text: impl Into<String>) -> Self {
        self.initial_text = Some(text.into());
        self
    }

    /// Execute select prompt with fuzzy search.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(T))` - User selected an option
    /// - `Ok(None)` - No options available or user cancelled (ESC / Ctrl+C)
    ///
    /// # Errors
    ///
    /// Returns an error if the fzf process fails to start or interact
    pub fn prompt(self) -> Result<Option<T>>
    where
        T: std::fmt::Display + Clone,
    {
        // Handle confirm case (bool options)
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<bool>() {
            return prompt_confirm(&self.message, self.default);
        }

        if self.options.is_empty() {
            return Ok(None);
        }

        // Strip ANSI codes and trim whitespace from display strings for fzf
        // compatibility. Trimming is required because fzf trims its output,
        // so we must trim the display strings consistently to allow match-back.
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).trim().to_string())
            .collect();

        let fzf = build_fzf(&self.message, self.help_message, self.initial_text.as_deref());

        let selected = run_with_output(fzf, display_options.iter().map(|s| s.as_str()));

        match selected {
            None => Ok(None),
            Some(s) => {
                let s = s.trim().to_string();
                let idx = display_options.iter().position(|d| d == &s);
                Ok(idx.and_then(|i| self.options.get(i).cloned()))
            }
        }
    }
}

impl<T> SelectBuilderOwned<T> {
    /// Set starting cursor position.
    ///
    /// Note: This is a no-op with fzf backend. fzf does not support pre-selecting
    /// a specific item by index.
    pub fn with_starting_cursor(self, _cursor: usize) -> Self {
        self
    }

    /// Set initial search text for fuzzy search.
    pub fn with_initial_text(mut self, text: impl Into<String>) -> Self {
        self.initial_text = Some(text.into());
        self
    }

    /// Execute select prompt with fuzzy search and owned values.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(T))` - User selected an option
    /// - `Ok(None)` - No options available or user cancelled (ESC / Ctrl+C)
    ///
    /// # Errors
    ///
    /// Returns an error if the fzf process fails to start or interact
    pub fn prompt(self) -> Result<Option<T>>
    where
        T: std::fmt::Display,
    {
        if self.options.is_empty() {
            return Ok(None);
        }

        // Strip ANSI codes and trim whitespace from display strings for fzf
        // compatibility. Trimming is required because fzf trims its output,
        // so we must trim the display strings consistently to allow match-back.
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).trim().to_string())
            .collect();

        let fzf = build_fzf(&self.message, None, self.initial_text.as_deref());

        let selected = run_with_output(fzf, display_options.iter().map(|s| s.as_str()));

        match selected {
            None => Ok(None),
            Some(s) => {
                let s = s.trim().to_string();
                let idx = display_options.iter().position(|d| d == &s);
                Ok(idx.and_then(|i| self.options.into_iter().nth(i)))
            }
        }
    }
}

/// Runs a yes/no confirmation prompt via fzf.
///
/// Returns `Ok(Some(true))` for Yes, `Ok(Some(false))` for No, and `Ok(None)` if cancelled.
fn prompt_confirm<T: 'static + Clone>(message: &str, default: Option<bool>) -> Result<Option<T>> {
    // Present "Yes" first when default is true (or unset), "No" first when default is false
    let items: Vec<&str> = if default == Some(false) {
        vec!["No", "Yes"]
    } else {
        vec!["Yes", "No"]
    };

    let fzf = build_fzf(message, None, None);
    let selected = run_with_output(fzf, items.iter().copied());

    let result: Option<bool> = match selected.as_deref().map(str::trim) {
        Some("Yes") => Some(true),
        Some("No") => Some(false),
        _ => None,
    };

    // Safe cast: caller guarantees T is bool (checked via TypeId at call site)
    Ok(result.map(|b| unsafe { std::mem::transmute_copy(&b) }))
}

/// Escapes a string for safe embedding as a shell single-quoted argument.
///
/// Single-quotes in the input are replaced with `'\''` (end quote, literal
/// single-quote, reopen quote) so the entire result can be wrapped in `'...'`.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Strips bracketed-paste escape sequences from a string.
///
/// When bracketed paste mode is active in the terminal, pasted text is wrapped
/// in `\x1b[200~` (start) and `\x1b[201~` (end) markers. This function removes
/// those markers from the captured shell output so the raw input value is clean.
fn strip_bracketed_paste(s: &str) -> String {
    s.replace("\x1b[200~", "").replace("\x1b[201~", "")
}

/// Builder for input prompts.
pub struct InputBuilder {
    message: String,
    allow_empty: bool,
    default: Option<String>,
    default_display: Option<String>,
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
    /// Delegates to `sh -c 'read -r VAR ...'` via `/dev/tty` so that terminal
    /// state issues caused by prior fzf invocations (raw mode, SIGCHLD, etc.)
    /// do not affect input reading. When `allow_empty` is false and no default
    /// is set, re-prompts until non-empty input is provided.
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
        // Determine what to show as the default hint (computed once, outside the loop)
        let hint = match (&self.default, &self.default_display) {
            (Some(val), Some(display)) if val != display => {
                // Masked default: show display (e.g. truncated API key), actual value is val
                Some(display.clone())
            }
            (Some(val), _) => Some(val.clone()),
            _ => None,
        };

        loop {
            // Build the prompt string shown to the user
            let prompt_str = match &hint {
                Some(h) => format!("{} [{}]: ", self.message, h),
                None => format!("{}: ", self.message),
            };

            // Use shell-native `read` to collect input from /dev/tty.
            // The prompt is printed to stderr (fd 2) so the user sees it even
            // when stdout is captured. `read -r` reads from /dev/tty directly,
            // bypassing any stdin buffering or terminal mode issues left by fzf.
            // The value is printed to stdout so we can capture it.
            let script = format!(
                "printf '%s' {prompt} >&2; read -r FORGE_INPUT </dev/tty && printf '%s' \"$FORGE_INPUT\"",
                prompt = shell_escape(&prompt_str),
            );

            let output = Command::new("sh")
                .arg("-c")
                .arg(&script)
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to spawn shell for input: {e}"))?;

            // Non-zero exit (e.g. Ctrl+C in the shell) → treat as cancellation
            if !output.status.success() {
                return Ok(None);
            }

            let raw = String::from_utf8_lossy(&output.stdout).to_string();
            // Strip bracketed-paste markers (\033[200~ ... \033[201~) that the
            // terminal injects around pasted text. We strip them here rather than
            // disabling bracketed-paste mode via escape sequences, which causes
            // unwanted screen clearing.
            let value = strip_bracketed_paste(&raw);
            let trimmed = value.trim();

            if trimmed.is_empty() {
                // User pressed Enter with no input
                if let Some(ref default_val) = self.default {
                    return Ok(Some(default_val.clone()));
                }
                if self.allow_empty {
                    return Ok(Some(String::new()));
                }
                // Empty input not allowed and no default — re-prompt
                let mut out = io::stdout();
                writeln!(out, "Input cannot be empty. Please try again.")?;
                continue;
            }

            return Ok(Some(trimmed.to_string()));
        }
    }
}

/// Builder for multi-select prompts.
pub struct MultiSelectBuilder<T> {
    message: String,
    options: Vec<T>,
}

impl<T> MultiSelectBuilder<T> {
    /// Execute multi-select prompt.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Vec<T>))` - User selected one or more options
    /// - `Ok(None)` - No options available or user cancelled (ESC / Ctrl+C)
    ///
    /// # Errors
    ///
    /// Returns an error if the fzf process fails to start or interact
    pub fn prompt(self) -> Result<Option<Vec<T>>>
    where
        T: std::fmt::Display + Clone,
    {
        if self.options.is_empty() {
            return Ok(None);
        }

        // Strip ANSI codes and trim whitespace from display strings for fzf
        // compatibility. Trimming is required because fzf trims its output,
        // so we must trim the display strings consistently to allow match-back.
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).trim().to_string())
            .collect();

        // Use fzf --multi for multi-selection; Tab selects items.
        // --height=40% keeps fzf inline (no alternate screen / no apparent clear).
        let fzf = Fzf::builder()
            .layout(Layout::Reverse)
            .header(self.message.as_str())
            .header_first(true)
            .custom_args(vec!["--multi".to_string(), "--height=40%".to_string()])
            .build()
            .expect("fzf builder should always succeed with default options");

        let mut fzf = fzf;
        fzf.run().map_err(|e| anyhow::anyhow!("Failed to start fzf: {e}"))?;
        fzf.add_items(display_options.iter().map(|s| s.as_str()))
            .map_err(|e| anyhow::anyhow!("Failed to add items to fzf: {e}"))?;

        // output() blocks until fzf exits; for --multi, the output contains
        // newline-separated selections
        let raw_output = fzf.output();

        match raw_output {
            None => Ok(None),
            Some(output) => {
                let selected_lines: Vec<String> = output
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();

                if selected_lines.is_empty() {
                    return Ok(None);
                }

                let selected_items: Vec<T> = selected_lines
                    .iter()
                    .filter_map(|sel| {
                        display_options
                            .iter()
                            .position(|d| d == sel)
                            .and_then(|i| self.options.get(i).cloned())
                    })
                    .collect();

                if selected_items.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(selected_items))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_select_builder_creates() {
        let builder = ForgeSelect::select("Test", vec!["a", "b", "c"]);
        assert_eq!(builder.message, "Test");
        assert_eq!(builder.options, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_confirm_builder_creates() {
        let builder = ForgeSelect::confirm("Confirm?");
        assert_eq!(builder.message, "Confirm?");
        assert_eq!(builder.options, vec![true, false]);
    }

    #[test]
    fn test_input_builder_creates() {
        let builder = ForgeSelect::input("Enter name:");
        assert_eq!(builder.message, "Enter name:");
        assert_eq!(builder.allow_empty, false);
    }

    #[test]
    fn test_multi_select_builder_creates() {
        let builder = ForgeSelect::multi_select("Select options:", vec!["a", "b", "c"]);
        assert_eq!(builder.message, "Select options:");
        assert_eq!(builder.options, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_select_builder_with_initial_text() {
        let builder =
            ForgeSelect::select("Test", vec!["apple", "banana", "cherry"]).with_initial_text("app");
        assert_eq!(builder.initial_text, Some("app".to_string()));
    }

    #[test]
    fn test_select_owned_builder_with_initial_text() {
        let builder = ForgeSelect::select_owned("Test", vec!["apple", "banana", "cherry"])
            .with_initial_text("ban");
        assert_eq!(builder.initial_text, Some("ban".to_string()));
    }

    #[test]
    fn test_ansi_stripping() {
        let options = ["\x1b[1mBold\x1b[0m", "\x1b[31mRed\x1b[0m"];
        let display: Vec<String> = options
            .iter()
            .map(|s| strip_ansi_codes(s).to_string())
            .collect();

        assert_eq!(display, vec!["Bold", "Red"]);
    }

    #[test]
    fn test_display_options_are_trimmed() {
        // Simulate a provider display string with leading spaces (like template providers)
        // and trailing spaces (like padded names). After stripping ANSI and trimming,
        // the result must match what fzf returns (fzf trims its output).
        let options = ["  openai               [empty]", "✓ anthropic            [api.anthropic.com]"];
        let display: Vec<String> = options
            .iter()
            .map(|s| strip_ansi_codes(s).trim().to_string())
            .collect();

        // Trimmed display options must match what fzf would return after its own trim
        assert_eq!(display[0], "openai               [empty]");
        assert_eq!(display[1], "✓ anthropic            [api.anthropic.com]");
    }

    #[test]
    fn test_with_starting_cursor_is_noop() {
        // with_starting_cursor should be a no-op and not panic
        let builder = ForgeSelect::select("Test", vec!["a", "b", "c"]).with_starting_cursor(2);
        assert_eq!(builder.message, "Test");
        assert_eq!(builder.options, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_input_builder_with_default() {
        let builder = ForgeSelect::input("Enter key:").with_default("mykey");
        assert_eq!(builder.default, Some("mykey".to_string()));
    }

    #[test]
    fn test_input_builder_allow_empty() {
        let builder = ForgeSelect::input("Enter:").allow_empty(true);
        assert_eq!(builder.allow_empty, true);
    }

    #[test]
    fn test_strip_bracketed_paste() {
        // Pasted text wrapped in bracketed-paste markers must be stripped
        let input = "\x1b[200~myapikey\x1b[201~";
        assert_eq!(strip_bracketed_paste(input), "myapikey");

        // Text without markers is returned unchanged
        let plain = "myapikey";
        assert_eq!(strip_bracketed_paste(plain), "myapikey");

        // Only start marker
        let only_start = "\x1b[200~myapikey";
        assert_eq!(strip_bracketed_paste(only_start), "myapikey");
    }
}
