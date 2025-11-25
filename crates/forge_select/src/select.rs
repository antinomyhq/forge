use anyhow::Result;
use console::strip_ansi_codes;

use crate::{ApplicationCursorKeysGuard, BracketedPasteGuard};

/// Centralized cliclack select functionality with consistent error handling
pub struct ForgeSelect;

/// Builder for select prompts with fuzzy search
#[derive(derive_setters::Setters)]
pub struct SelectBuilder<T> {
    message: String,
    options: Vec<T>,
    #[setters(strip_option)]
    starting_cursor: Option<usize>,
    #[setters(strip_option)]
    default: Option<bool>,
    #[setters(strip_option)]
    help_message: Option<&'static str>,
    #[setters(strip_option)]
    max_rows: Option<usize>,
}

/// Builder for select prompts that takes ownership (doesn't require Clone)
#[derive(derive_setters::Setters)]
pub struct SelectBuilderOwned<T> {
    message: String,
    options: Vec<T>,
    #[setters(strip_option)]
    starting_cursor: Option<usize>,
    #[setters(strip_option)]
    max_rows: Option<usize>,
}

impl ForgeSelect {
    /// Entry point for select operations with fuzzy search
    pub fn select<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilder<T> {
        SelectBuilder {
            message: message.into(),
            options,
            starting_cursor: None,
            default: None,
            help_message: None,
            max_rows: None,
        }
    }

    /// Entry point for select operations with owned values (doesn't require
    /// Clone)
    pub fn select_owned<T>(message: impl Into<String>, options: Vec<T>) -> SelectBuilderOwned<T> {
        SelectBuilderOwned {
            message: message.into(),
            options,
            starting_cursor: None,
            max_rows: None,
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
            max_rows: None,
        }
    }

    /// Prompt a question and get text input
    pub fn input(message: impl Into<String>) -> InputBuilder {
        InputBuilder { message: message.into(), allow_empty: false, default: None }
    }

    /// Multi-select prompt
    pub fn multi_select<T>(message: impl Into<String>, options: Vec<T>) -> MultiSelectBuilder<T> {
        MultiSelectBuilder { message: message.into(), options }
    }
}

impl<T: 'static> SelectBuilder<T> {
    /// Execute select prompt with fuzzy search
    ///
    /// # Returns
    ///
    /// - `Ok(Some(T))` - User selected an option
    /// - `Ok(None)` - No options available or user cancelled (ESC)
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal interaction fails for reasons other
    /// than user cancellation
    pub fn prompt(self) -> Result<Option<T>>
    where
        T: std::fmt::Display + Clone,
    {
        // Disable bracketed paste mode to prevent ~0 and ~1 markers
        let _paste_guard = BracketedPasteGuard::new()?;
        // Disable application cursor keys to ensure arrow keys work correctly
        let _cursor_guard = ApplicationCursorKeysGuard::new()?;
        // Handle confirm case (bool options)
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<bool>() {
            let mut confirm = cliclack::confirm(&self.message);

            if let Some(default) = self.default {
                confirm = confirm.initial_value(default);
            }

            let result = match confirm.interact() {
                Ok(value) => Some(value),
                Err(_) => return Ok(None), // User cancelled (ESC)
            };
            // Safe cast since we checked the type
            return Ok(result.map(|b| unsafe { std::mem::transmute_copy(&b) }));
        }

        // Select for regular options
        if self.options.is_empty() {
            return Ok(None);
        }

        // Strip ANSI codes from display strings for better fuzzy search experience
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).to_string())
            .collect();

        let mut select = cliclack::select(&self.message).filter_mode();

        // Limit visible rows to prevent scrolling in long lists
        if let Some(max_rows) = self.max_rows {
            select = select.max_rows(max_rows);
        }

        // Add all items
        for (idx, display) in display_options.iter().enumerate() {
            select = select.item(idx, display, "");
        }

        // Set initial value if starting cursor is provided
        // Note: In filter mode with long lists, this causes scrolling to that
        // position Consider using max_rows() to limit scrolling
        if let Some(cursor) = self.starting_cursor {
            if cursor < self.options.len() {
                select = select.initial_value(cursor);
            }
        }

        let idx_opt = match select.interact() {
            Ok(idx) => Some(idx),
            Err(_) => return Ok(None), // User cancelled (ESC)
        };

        Ok(idx_opt.and_then(|idx| self.options.get(idx).cloned()))
    }
}

impl<T> SelectBuilderOwned<T> {
    /// Execute select prompt with fuzzy search and owned values
    ///
    /// # Returns
    ///
    /// - `Ok(Some(T))` - User selected an option
    /// - `Ok(None)` - No options available or user cancelled (ESC)
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

        // Strip ANSI codes from display strings for better fuzzy search experience
        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).to_string())
            .collect();

        let mut select = cliclack::select(&self.message).filter_mode();

        // Limit visible rows to prevent scrolling in long lists
        if let Some(max_rows) = self.max_rows {
            select = select.max_rows(max_rows);
        }

        // Add all items
        for (idx, display) in display_options.iter().enumerate() {
            select = select.item(idx, display, "");
        }

        // Set initial value if starting cursor is provided
        // Note: In filter mode with long lists, this causes scrolling to that
        // position Consider using max_rows() to limit scrolling
        if let Some(cursor) = self.starting_cursor {
            if cursor < self.options.len() {
                select = select.initial_value(cursor);
            }
        }

        let idx_opt = match select.interact() {
            Ok(idx) => Some(idx),
            Err(_) => return Ok(None), // User cancelled (ESC)
        };

        Ok(idx_opt.and_then(|idx| self.options.into_iter().nth(idx)))
    }
}

/// Builder for input prompts
pub struct InputBuilder {
    message: String,
    allow_empty: bool,
    default: Option<String>,
}

impl InputBuilder {
    /// Allow empty input
    pub fn allow_empty(mut self, allow: bool) -> Self {
        self.allow_empty = allow;
        self
    }

    /// Set default value
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Execute input prompt
    ///
    /// # Returns
    ///
    /// - `Ok(Some(String))` - User provided input
    /// - `Ok(None)` - User cancelled (ESC)
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

        let mut input = cliclack::input(&self.message);

        if let Some(default) = &self.default {
            input = input.placeholder(default);
        }

        if !self.allow_empty {
            input = input.validate(|value: &String| {
                if value.is_empty() {
                    Err("Input cannot be empty")
                } else {
                    Ok(())
                }
            });
        }

        match input.interact::<String>() {
            Ok(value) => {
                // If value is empty and we have a default, use the default
                if value.is_empty() && self.default.is_some() {
                    Ok(self.default)
                } else {
                    Ok(Some(value))
                }
            }
            Err(_) => Ok(None), // User cancelled (ESC)
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
    /// - `Ok(None)` - No options available or user cancelled (ESC)
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

        let mut multi_select = cliclack::multiselect(&self.message).filter_mode();

        // Add all items
        for (idx, option) in self.options.iter().enumerate() {
            multi_select = multi_select.item(idx, option.to_string(), "");
        }

        let indices_opt = match multi_select.interact() {
            Ok(indices) => Some(indices),
            Err(_) => return Ok(None), // User cancelled (ESC)
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
    fn test_select_builder_with_max_rows() {
        let builder = ForgeSelect::select("Test", vec!["apple", "banana", "cherry"]).max_rows(10);
        assert_eq!(builder.max_rows, Some(10));
    }

    #[test]
    fn test_select_owned_builder_with_max_rows() {
        let builder =
            ForgeSelect::select_owned("Test", vec!["apple", "banana", "cherry"]).max_rows(15);
        assert_eq!(builder.max_rows, Some(15));
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
