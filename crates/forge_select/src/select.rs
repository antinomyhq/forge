use anyhow::Result;
use console::{strip_ansi_codes, style};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect, Input, MultiSelect};

use crate::{ApplicationCursorKeysGuard, BracketedPasteGuard};

/// Check if a dialoguer error is an interrupted error (CTRL+C)
fn is_interrupted_error(err: &dialoguer::Error) -> bool {
    match err {
        dialoguer::Error::IO(error) => error.kind() == std::io::ErrorKind::Interrupted,
    }
}

/// Centralized dialoguer select functionality with consistent error handling
pub struct ForgeSelect;

/// Builder for select prompts with fuzzy search
pub struct SelectBuilder<T> {
    message: String,
    options: Vec<T>,
    starting_cursor: Option<usize>,
    default: Option<bool>,
    help_message: Option<&'static str>,
    initial_text: Option<String>,
}

/// Builder for select prompts that takes ownership (doesn't require Clone)
pub struct SelectBuilderOwned<T> {
    message: String,
    options: Vec<T>,
    starting_cursor: Option<usize>,
    initial_text: Option<String>,
}

impl ForgeSelect {
    /// Create a consistent theme for all select operations without prompt
    /// suffix arrow
    fn default_theme() -> ColorfulTheme {
        ColorfulTheme {
            prompt_suffix: style("".to_string()),
            ..ColorfulTheme::default()
        }
    }

    /// Entry point for select operations with fuzzy search
    pub fn select<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilder<T> {
        SelectBuilder {
            message: message.into(),
            options,
            starting_cursor: None,
            default: None,
            help_message: None,
            initial_text: None,
        }
    }

    /// Entry point for select operations with owned values (doesn't require
    /// Clone)
    pub fn select_owned<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilderOwned<T> {
        SelectBuilderOwned {
            message: message.into(),
            options,
            starting_cursor: None,
            initial_text: None,
        }
    }

    /// Convenience method for confirm (yes/no)
    pub fn confirm(message: impl Into<String>) -> SelectBuilder<bool> {
        SelectBuilder {
            message: message.into(),
            options: vec![true, false],
            starting_cursor: None,
            default: None,
            help_message: None,
            initial_text: None,
        }
    }

    /// Prompt a question and get text input
    pub fn input(message: impl Into<String>) -> InputBuilder {
        InputBuilder {
            message: message.into(),
            allow_empty: false,
            default_value: None,
            default_display: None,
            show_default: true,
        }
    }

    /// Multi-select prompt
    pub fn multi_select<T>(message: impl Into<String>, options: Vec<T>) -> MultiSelectBuilder<T> {
        MultiSelectBuilder { message: message.into(), options }
    }
}

impl<T: 'static> SelectBuilder<T> {
    /// Set starting cursor position
    pub fn with_starting_cursor(mut self, cursor: usize) -> Self {
        self.starting_cursor = Some(cursor);
        self
    }

    /// Set default for confirm (only works with bool options)
    pub fn with_default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    /// Set help message
    pub fn with_help_message(mut self, message: &'static str) -> Self {
        self.help_message = Some(message);
        self
    }

    /// Set initial search text for fuzzy search
    pub fn with_initial_text(mut self, text: impl Into<String>) -> Self {
        self.initial_text = Some(text.into());
        self
    }

    /// Execute select prompt with fuzzy search
    ///
    /// # Returns
    ///
    /// - `Ok(Some(T))` - User selected an option
    /// - `Ok(None)` - No options available or user cancelled (CTRL+C)
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal interaction fails for reasons other
    /// than user cancellation
    pub fn prompt(self) -> Result<Option<T>>
    where
        T: std::fmt::Display + Clone,
    {
        // Handle confirm case (bool options)
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<bool>() {
            let theme = ForgeSelect::default_theme();
            let mut confirm = Confirm::with_theme(&theme).with_prompt(&self.message);

            if let Some(default) = self.default {
                confirm = confirm.default(default);
            }

            let result = match confirm.interact_opt() {
                Ok(value) => value,
                Err(e) if is_interrupted_error(&e) => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            // Safe cast since we checked the type
            return Ok(result.map(|b| unsafe { std::mem::transmute_copy(&b) }));
        }

        // FuzzySelect for regular options
        if self.options.is_empty() {
            return Ok(None);
        }

        // Disable bracketed paste mode to prevent ~0 and ~1 markers during
        // fuzzy search input
        let _paste_guard = BracketedPasteGuard::new()?;
        // Disable application cursor keys to ensure arrow keys work correctly
        let _cursor_guard = ApplicationCursorKeysGuard::new()?;

        let theme = ForgeSelect::default_theme();

        // Strip ANSI codes from display strings for better fuzzy search experience
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).to_string())
            .collect();

        let mut select = FuzzySelect::with_theme(&theme)
            .with_prompt(&self.message)
            .items(&display_options);

        if let Some(cursor) = self.starting_cursor {
            select = select.default(cursor);
        } else {
            select = select.default(0);
        }

        if let Some(text) = self.initial_text {
            select = select.with_initial_text(text);
        }

        let idx_opt = match select.interact_opt() {
            Ok(value) => value,
            Err(e) if is_interrupted_error(&e) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(idx_opt.and_then(|idx| self.options.get(idx).cloned()))
    }
}

impl<T> SelectBuilderOwned<T> {
    /// Set starting cursor position
    pub fn with_starting_cursor(mut self, cursor: usize) -> Self {
        self.starting_cursor = Some(cursor);
        self
    }

    /// Set initial search text for fuzzy search
    pub fn with_initial_text(mut self, text: impl Into<String>) -> Self {
        self.initial_text = Some(text.into());
        self
    }

    /// Execute select prompt with fuzzy search and owned values
    ///
    /// # Returns
    ///
    /// - `Ok(Some(T))` - User selected an option
    /// - `Ok(None)` - No options available or user cancelled (CTRL+C)
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal interaction fails for reasons other
    /// than user cancellation
    pub fn prompt(self) -> Result<Option<T>>
    where
        T: std::fmt::Display,
    {
        if self.options.is_empty() {
            return Ok(None);
        }

        // Disable bracketed paste mode to prevent ~0 and ~1 markers during
        // fuzzy search input
        let _paste_guard = BracketedPasteGuard::new()?;
        // Disable application cursor keys to ensure arrow keys work correctly
        let _cursor_guard = ApplicationCursorKeysGuard::new()?;

        let theme = ForgeSelect::default_theme();

        // Strip ANSI codes from display strings for better fuzzy search experience
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).to_string())
            .collect();

        let mut select = FuzzySelect::with_theme(&theme)
            .with_prompt(&self.message)
            .items(&display_options);

        if let Some(cursor) = self.starting_cursor {
            select = select.default(cursor);
        } else {
            select = select.default(0);
        }

        if let Some(text) = self.initial_text {
            select = select.with_initial_text(text);
        }

        let idx_opt = match select.interact_opt() {
            Ok(value) => value,
            Err(e) if is_interrupted_error(&e) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        Ok(idx_opt.and_then(|idx| self.options.into_iter().nth(idx)))
    }
}

/// Builder for input prompts
pub struct InputBuilder {
    message: String,
    allow_empty: bool,
    default_value: Option<String>,
    default_display: Option<String>,
    show_default: bool,
}

// Internal type for dialoguer interaction
struct DialoguerMaskedDefault {
    value: String,
    display: String,
}

impl std::fmt::Display for DialoguerMaskedDefault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display)
    }
}

impl std::str::FromStr for DialoguerMaskedDefault {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DialoguerMaskedDefault {
            value: s.to_string(),
            display: s.to_string(),
        })
    }
}

impl Clone for DialoguerMaskedDefault {
    fn clone(&self) -> Self {
        DialoguerMaskedDefault {
            value: self.value.clone(),
            display: self.display.clone(),
        }
    }
}

impl InputBuilder {
    /// Allow empty input
    pub fn allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    /// Set default value
    ///
    /// Accepts any type that implements `Display + AsRef<str>`:
    /// - `Display` is used for showing in the prompt
    /// - `AsRef<str>` is used for the actual value
    ///
    /// For regular strings, both are the same.
    /// For types like `ApiKey`, Display can show a masked version while AsRef<str> returns the actual value.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Regular default (Display and AsRef<str> are the same)
    /// ForgeSelect::input("Enter name:")
    ///     .with_default("John")
    ///     .prompt()?;
    ///
    /// // Masked default (e.g., ApiKey with Display showing masked version)
    /// ForgeSelect::input("Enter API key:")
    ///     .with_default(&api_key)  // ApiKey implements Display (masked) and AsRef<str> (actual)
    ///     .prompt()?;
    /// ```
    pub fn with_default<T>(mut self, default: T) -> Self
    where
        T: std::fmt::Display + AsRef<str>,
    {
        self.default_value = Some(default.as_ref().to_string());
        self.default_display = Some(default.to_string());
        self.show_default = true;
        self
    }

    /// Control whether the default value is displayed in the prompt
    ///
    /// When set to `false`, the default value will still be used when the user
    /// presses Enter without typing anything, but it won't be shown in the prompt.
    /// This is useful for sensitive data like API keys where you want to use a
    /// default but not display it.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let input = ForgeSelect::input("Enter API key (current: sk-...abc)")
    ///     .with_default("sk-1234567890abcdef")  // Full key used as default
    ///     .show_default(false)                   // But not displayed
    ///     .prompt()?;
    /// ```
    pub fn show_default(mut self, show: bool) -> Self {
        self.show_default = show;
        self
    }

    /// Execute input prompt
    ///
    /// # Returns
    ///
    /// - `Ok(Some(String))` - User provided input
    /// - `Ok(None)` - User cancelled (CTRL+C)
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal interaction fails for reasons other
    /// than user cancellation
    pub fn prompt(self) -> Result<Option<String>> {
        // Disable bracketed paste mode to prevent ~0 and ~1 markers during input
        let _paste_guard = BracketedPasteGuard::new()?;
        // Disable application cursor keys to ensure arrow keys work correctly
        let _cursor_guard = ApplicationCursorKeysGuard::new()?;

        let theme = ForgeSelect::default_theme();

        // Check if we have both value and display (masked default scenario)
        if let (Some(value), Some(display)) = (self.default_value, self.default_display) {
            // If value and display are different, use DialoguerMaskedDefault
            if value != display {
                let masked = DialoguerMaskedDefault { value, display };
                let input = Input::with_theme(&theme)
                    .with_prompt(&self.message)
                    .allow_empty(self.allow_empty)
                    .default(masked);

                return match input.interact_text() {
                    Ok(masked_result) => Ok(Some(masked_result.value)),
                    Err(e) if is_interrupted_error(&e) => Ok(None),
                    Err(e) => Err(e.into()),
                };
            }
            
            // If they're the same, treat as normal string
            let input = Input::with_theme(&theme)
                .with_prompt(&self.message)
                .allow_empty(self.allow_empty)
                .show_default(self.show_default)
                .default(value);

            return match input.interact_text() {
                Ok(value) => Ok(Some(value)),
                Err(e) if is_interrupted_error(&e) => Ok(None),
                Err(e) => Err(e.into()),
            };
        }

        // No default provided
        let input = Input::with_theme(&theme)
            .with_prompt(&self.message)
            .allow_empty(self.allow_empty);

        match input.interact_text() {
            Ok(value) => Ok(Some(value)),
            Err(e) if is_interrupted_error(&e) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Builder for multi-select prompts
pub struct MultiSelectBuilder<T> {
    message: String,
    options: Vec<T>,
}

impl<T> MultiSelectBuilder<T> {
    /// Execute multi-select prompt
    ///
    /// # Returns
    ///
    /// - `Ok(Some(Vec<T>))` - User selected one or more options
    /// - `Ok(None)` - No options available or user cancelled (CTRL+C)
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal interaction fails for reasons other
    /// than user cancellation
    pub fn prompt(self) -> Result<Option<Vec<T>>>
    where
        T: std::fmt::Display + Clone,
    {
        if self.options.is_empty() {
            return Ok(None);
        }

        // Disable bracketed paste mode to prevent ~0 and ~1 markers
        let _paste_guard = BracketedPasteGuard::new()?;
        // Disable application cursor keys to ensure arrow keys work correctly
        let _cursor_guard = ApplicationCursorKeysGuard::new()?;

        let theme = ForgeSelect::default_theme();
        let multi_select = MultiSelect::with_theme(&theme)
            .with_prompt(&self.message)
            .items(&self.options);

        let indices_opt = match multi_select.interact_opt() {
            Ok(value) => value,
            Err(e) if is_interrupted_error(&e) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        Ok(indices_opt.map(|indices| {
            indices
                .into_iter()
                .filter_map(|idx| self.options.get(idx).cloned())
                .collect()
        }))
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
}
