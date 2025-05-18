use forge_domain::{ToolOutput, ToolOutputValue};

pub trait ToolContentExtension {
    fn into_string(self) -> String;
}

impl ToolContentExtension for ToolOutput {
    /// To be used only in tests to convert the ToolContent into a string
    fn into_string(self) -> String {
        let ToolOutput { values: items, .. } = self;
        items
            .into_iter()
            .filter_map(|item| match item {
                ToolOutputValue::Text(text) => Some(text),
                ToolOutputValue::Base64URL(_) => None,
                ToolOutputValue::Empty => None,
            })
            .collect()
    }
}
