use forge_domain::{ToolContent, ToolOutput};

pub trait ToolContentExtension {
    fn into_string(self) -> String;
}

impl ToolContentExtension for ToolContent {
    /// To be used only in tests to convert the ToolContent into a string
    fn into_string(self) -> String {
        let ToolContent { items, .. } = self;
        items
            .into_iter()
            .filter_map(|item| match item {
                ToolOutput::Text(text) => Some(text),
                ToolOutput::Base64URL(_) => None,
                ToolOutput::Empty => None,
            })
            .collect()
    }
}
