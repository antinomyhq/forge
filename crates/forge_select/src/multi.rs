use std::io::IsTerminal;

use anyhow::Result;
use console::strip_ansi_codes;
use fzf_wrapped::{Fzf, Layout};

use crate::select::{indexed_items, parse_fzf_index};

/// Builder for multi-select prompts.
pub struct MultiSelectBuilder<T> {
    pub(crate) message: String,
    pub(crate) options: Vec<T>,
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
        // Bail immediately when stdin is not a terminal to prevent the process
        // from blocking indefinitely on a detached or non-interactive session.
        if !std::io::stdin().is_terminal() {
            return Ok(None);
        }

        if self.options.is_empty() {
            return Ok(None);
        }

        let display_options: Vec<String> = self
            .options
            .iter()
            .map(|item| strip_ansi_codes(&item.to_string()).trim().to_string())
            .collect();

        let fzf = build_multi_fzf(&self.message);

        let mut fzf = fzf;
        fzf.run()
            .map_err(|e| anyhow::anyhow!("Failed to start fzf: {e}"))?;
        fzf.add_items(indexed_items(&display_options))
            .map_err(|e| anyhow::anyhow!("Failed to add items to fzf: {e}"))?;

        let raw_output = fzf.output();

        match raw_output {
            None => Ok(None),
            Some(output) => {
                let selected_items: Vec<T> = output
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .filter_map(|line| {
                        parse_fzf_index(line).and_then(|index| self.options.get(index).cloned())
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

/// Builds an `Fzf` instance for multi-select prompts.
fn build_multi_fzf(message: &str) -> Fzf {
    let mut builder = Fzf::builder();
    builder.layout(Layout::Reverse);
    builder.no_scrollbar(true);
    builder.prompt(format!("{} ❯ ", message));
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
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::ForgeWidget;

    #[test]
    fn test_multi_select_builder_creates() {
        let builder = ForgeWidget::multi_select("Select options:", vec!["a", "b", "c"]);
        assert_eq!(builder.message, "Select options:");
        assert_eq!(builder.options, vec!["a", "b", "c"]);
    }
}
