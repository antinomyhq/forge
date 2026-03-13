use std::io::{self, Write};
use std::process::{Command, Stdio};

use anyhow::Result;
use colored::Colorize;
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
    starting_cursor: Option<usize>,
    default: Option<bool>,
    help_message: Option<&'static str>,
    initial_text: Option<String>,
    header_lines: usize,
}

/// Builder for select prompts that takes ownership (doesn't require Clone).
pub struct SelectBuilderOwned<T> {
    message: String,
    options: Vec<T>,
    starting_cursor: Option<usize>,
    initial_text: Option<String>,
}

impl ForgeSelect {
    /// Entry point for select operations with fuzzy search.
    pub fn select<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilder<T> {
        SelectBuilder {
            message: message.into(),
            options,
            starting_cursor: None,
            default: None,
            help_message: None,
            initial_text: None,
            header_lines: 0,
        }
    }

    /// Entry point for select operations with owned values (doesn't require
    /// Clone).
    pub fn select_owned<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilderOwned<T> {
        SelectBuilderOwned {
            message: message.into(),
            options,
            starting_cursor: None,
            initial_text: None,
        }
    }

    /// Convenience method for confirm (yes/no).
    pub fn confirm(message: impl Into<String>) -> SelectBuilder<bool> {
        SelectBuilder {
            message: message.into(),
            options: vec![true, false],
            starting_cursor: None,
            default: None,
            help_message: None,
            initial_text: None,
            header_lines: 0,
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
/// `--height=80%` is always added so fzf runs inline (below the current cursor)
/// rather than switching to the alternate screen buffer. Without this flag fzf
/// uses full-screen mode which enters the alternate screen (`\033[?1049h`),
/// making it appear as though the terminal is cleared. 80% matches the shell
/// plugin's `_forge_fzf` wrapper for a consistent UI.
///
/// Items are always passed as `"{idx}\t{display}"` and fzf is configured with
/// `--delimiter=\t --with-nth=2..` so only the display portion is shown. The
/// index prefix survives in fzf's output and is parsed back to look up the
/// original item by position — this avoids the `position()` ambiguity when
/// multiple items have identical display strings after ANSI stripping.
///
/// When `starting_cursor` is provided, `--bind="load:pos(N)"` is added so fzf
/// pre-positions the cursor on the Nth item (1-based in fzf's `pos()` action).
/// The `load` event is used instead of `start` because items are written to
/// fzf's stdin after the process starts.
///
/// The flags `--exact`, `--cycle`, `--select-1`, `--no-scrollbar`, and
/// `--color=dark,header:bold` mirror the shell plugin's `_forge_fzf` wrapper
/// for a consistent user experience across both entry points.
///
/// The `message` is used as the fzf `--prompt` so the prompt line reads
/// `"Select a model: "` instead of the default `"> "`, placing the question
/// inline with the search cursor (e.g. `Select a model: ❯`). If a
/// `help_message` is provided it is shown as a `--header` above the list.
fn build_fzf(
    message: &str,
    help_message: Option<&str>,
    initial_text: Option<&str>,
    starting_cursor: Option<usize>,
    header_lines: usize,
) -> Fzf {
    let mut builder = Fzf::builder();
    builder.layout(Layout::Reverse);
    builder.no_scrollbar(true);

    // Place the message on the prompt line: "Select a model: ❯ [search]"
    builder.prompt(format!("{} ❯ ", message));

    // Show optional help text as a header above the list.
    if let Some(help) = help_message {
        builder.header(help);
    }

    // Combine all custom args in a single call — custom_args() replaces (not
    // appends).
    // Flags mirror the shell plugin's `_forge_fzf` wrapper:
    //   --height=80%        inline display at 80% terminal height
    //   --exact             exact (non-fuzzy) matching
    //   --cycle             cursor wraps at top/bottom
    //   --select-1          auto-select when only one match remains
    //   --color=dark,header:bold  bold header text (extends the default dark theme)
    //   --delimiter=\t / --with-nth=2..  index-based item lookup
    let mut args = vec![
        "--height=80%".to_string(),
        "--exact".to_string(),
        "--cycle".to_string(),
        "--select-1".to_string(),
        "--color=dark,header:bold".to_string(),
        // Use fzf 0.70's default pointer (▌) — fzf-wrapped hardcodes ">" which
        // differs from the shell plugin that omits --pointer and gets ▌.
        "--pointer=▌".to_string(),
        "--delimiter=\t".to_string(),
        "--with-nth=2..".to_string(),
    ];
    if let Some(query) = initial_text {
        args.push(format!("--query={}", query));
    }
    // fzf's pos() action is 1-based; our starting_cursor is 0-based.
    // Use the `load` event (not `start`) because items are written to fzf's
    // stdin after the process starts — `start` fires before items arrive so
    // pos() has nothing to move to, while `load` fires once all items are
    // available.
    if let Some(cursor) = starting_cursor {
        args.push(format!("--bind=load:pos({})", cursor + 1));
    }
    if header_lines > 0 {
        args.push(format!("--header-lines={}", header_lines));
    }
    builder.custom_args(args);

    builder
        .build()
        .expect("fzf builder should always succeed with default options")
}

/// Formats items as `"{idx}\t{display}"` for passing to fzf.
///
/// The index prefix lets us recover the original position from fzf's output
/// without relying on string matching, which breaks when multiple items have
/// the same display string.
fn indexed_items(display_options: &[String]) -> Vec<String> {
    display_options
        .iter()
        .enumerate()
        .map(|(i, d)| format!("{}\t{}", i, d))
        .collect()
}

/// Parses the index from a line returned by fzf when items were formatted with
/// `indexed_items`. Returns `None` if the line is malformed.
fn parse_fzf_index(line: &str) -> Option<usize> {
    line.split('\t').next()?.trim().parse().ok()
}

impl<T: 'static> SelectBuilder<T> {
    /// Set starting cursor position.
    pub fn with_starting_cursor(mut self, cursor: usize) -> Self {
        self.starting_cursor = Some(cursor);
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

    /// Set the number of header lines (non-selectable) at the top of the list.
    ///
    /// When set to `n`, the first `n` items are displayed as a fixed header
    /// that is always visible but cannot be selected. Mirrors fzf's
    /// `--header-lines` flag, matching the shell plugin's porcelain output
    /// where the first line contains column headings.
    pub fn with_header_lines(mut self, n: usize) -> Self {
        self.header_lines = n;
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
        // compatibility. Trimming is required because fzf trims its output.
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).trim().to_string())
            .collect();

        let fzf = build_fzf(
            &self.message,
            self.help_message,
            self.initial_text.as_deref(),
            self.starting_cursor,
            self.header_lines,
        );

        // Prefix each item with its index so fzf's output can be mapped back
        // to the original item by position rather than by string matching.
        let selected = run_with_output(fzf, indexed_items(&display_options));

        match selected {
            None => Ok(None),
            Some(s) if s.trim().is_empty() => Ok(None),
            Some(s) => Ok(parse_fzf_index(&s).and_then(|i| self.options.get(i).cloned())),
        }
    }
}

impl<T> SelectBuilderOwned<T> {
    /// Set starting cursor position.
    pub fn with_starting_cursor(mut self, cursor: usize) -> Self {
        self.starting_cursor = Some(cursor);
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
        // compatibility. Trimming is required because fzf trims its output.
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).trim().to_string())
            .collect();

        let fzf = build_fzf(
            &self.message,
            None,
            self.initial_text.as_deref(),
            self.starting_cursor,
            0,
        );

        // Prefix each item with its index so fzf's output can be mapped back
        // to the original item by position rather than by string matching.
        let selected = run_with_output(fzf, indexed_items(&display_options));

        match selected {
            None => Ok(None),
            Some(s) if s.trim().is_empty() => Ok(None),
            Some(s) => Ok(parse_fzf_index(&s).and_then(|i| self.options.into_iter().nth(i))),
        }
    }
}

/// Runs a yes/no confirmation prompt via fzf.
///
/// Returns `Ok(Some(true))` for Yes, `Ok(Some(false))` for No, and `Ok(None)`
/// if cancelled.
fn prompt_confirm<T: 'static + Clone>(message: &str, default: Option<bool>) -> Result<Option<T>> {
    let items = ["Yes", "No"];

    // Pre-position cursor on the default option: "Yes" is index 0, "No" is index 1.
    let starting_cursor = if default == Some(false) {
        Some(1)
    } else {
        Some(0)
    };

    let fzf = build_fzf(message, None, None, starting_cursor, 0);
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
                Some(h) => format!(
                    "{} {} {}: ",
                    "?".yellow().bold(),
                    self.message.bold(),
                    format!("({})", h).dimmed(),
                ),
                None => format!("{} {}: ", "?".yellow().bold(), self.message.bold()),
            };

            // Use shell-native `read` to collect input from /dev/tty.
            // The prompt is printed to stderr (fd 2) so the user sees it even
            // when stdout is captured. Bash `read -e -r` enables Readline,
            // restoring cursor movement keys such as left/right arrows while
            // still bypassing any stdin buffering or terminal mode issues left
            // by fzf. The value is printed to stdout so we can capture it.
            let script = build_input_prompt_script(&prompt_str);

            let output = Command::new("bash")
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
        // compatibility. Trimming is required because fzf trims its output.
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).trim().to_string())
            .collect();

        // Use fzf --multi for multi-selection; Tab selects items.
        // Flags mirror the shell plugin's `_forge_fzf` wrapper for consistent UI.
        // --delimiter and --with-nth enable index-based lookup (same as single-select).
        // The message is placed on the prompt line (same as single-select).
        let fzf = {
            let mut builder = Fzf::builder();
            builder.layout(Layout::Reverse);
            builder.no_scrollbar(true);
            builder.prompt(format!("{} ❯ ", self.message));
            builder.custom_args(vec![
                "--height=80%".to_string(),
                "--exact".to_string(),
                "--cycle".to_string(),
                "--color=dark,header:bold".to_string(),
                "--pointer=▌".to_string(),
                "--delimiter=\t".to_string(),
                "--with-nth=2..".to_string(),
                "--multi".to_string(),
            ]);
            builder
                .build()
                .expect("fzf builder should always succeed with default options")
        };

        let mut fzf = fzf;
        fzf.run()
            .map_err(|e| anyhow::anyhow!("Failed to start fzf: {e}"))?;
        // Prefix each item with its index for position-based lookup on output.
        fzf.add_items(indexed_items(&display_options))
            .map_err(|e| anyhow::anyhow!("Failed to add items to fzf: {e}"))?;

        // output() blocks until fzf exits; for --multi, the output contains
        // newline-separated selections, each prefixed with the item index.
        let raw_output = fzf.output();

        match raw_output {
            None => Ok(None),
            Some(output) => {
                let selected_items: Vec<T> = output
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .filter_map(|line| {
                        parse_fzf_index(line).and_then(|i| self.options.get(i).cloned())
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
    fn test_indexed_items() {
        let display = vec![
            "Apple".to_string(),
            "Apple".to_string(),
            "Banana".to_string(),
        ];
        let actual = indexed_items(&display);
        let expected = vec!["0\tApple", "1\tApple", "2\tBanana"];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_fzf_index() {
        // Normal case: index\tdisplay
        assert_eq!(parse_fzf_index("0\tApple"), Some(0));
        assert_eq!(parse_fzf_index("2\tBanana"), Some(2));
        // Duplicate display strings are disambiguated by index
        assert_eq!(parse_fzf_index("1\tApple"), Some(1));
        // Malformed input returns None
        assert_eq!(parse_fzf_index("notanindex\tApple"), None);
        assert_eq!(parse_fzf_index(""), None);
    }

    #[test]
    fn test_display_options_are_trimmed() {
        // Leading/trailing whitespace (from provider display formatting) is
        // stripped before passing to fzf.
        let options = [
            "  openai               [empty]",
            "✓ anthropic            [api.anthropic.com]",
        ];
        let display: Vec<String> = options
            .iter()
            .map(|s| strip_ansi_codes(s).trim().to_string())
            .collect();

        assert_eq!(display[0], "openai               [empty]");
        assert_eq!(display[1], "✓ anthropic            [api.anthropic.com]");
    }

    #[test]
    fn test_with_starting_cursor() {
        let builder = ForgeSelect::select("Test", vec!["a", "b", "c"]).with_starting_cursor(2);
        assert_eq!(builder.starting_cursor, Some(2));
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
        let expected = "printf '%s' '? Enter John'\\''s key: ' >&2; read -e -r FORGE_INPUT </dev/tty && printf '%s' \"$FORGE_INPUT\""
            .to_string();
        assert_eq!(actual, expected);
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
