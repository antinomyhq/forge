use forge_domain::{ToolContent, ToolContentItem};

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
                ToolContentItem::Text(text) => Some(text),
                ToolContentItem::Base64URL(_) => None,
                ToolContentItem::Empty => None,
            })
            .collect()
    }
}
