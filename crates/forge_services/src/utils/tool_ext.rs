use forge_domain::{ToolContent, ToolContentItem};

pub trait ToolContentExtension {
    fn into_string(self) -> String;
}

impl ToolContentExtension for ToolContent {
    fn into_string(self) -> String {
        match self {
            ToolContent { items, .. } => items
                .into_iter()
                .map(|item| match item {
                    ToolContentItem::Text(text) => text,
                    ToolContentItem::Base64URL(image) => image,
                })
                .collect(),
        }
    }
}
