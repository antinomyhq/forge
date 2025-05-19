use forge_domain::{AttachmentContent, Image, ToolOutput, ToolOutputValue};

pub trait ToolContentExtension {
    fn into_string(self) -> String;
    fn contains(&self, needle: &str) -> bool;
}

impl ToolContentExtension for ToolOutput {
    /// To be used only in tests to convert the ToolContent into a string
    fn into_string(self) -> String {
        let ToolOutput { values: items, .. } = self;
        items
            .into_iter()
            .filter_map(|item| match item {
                ToolOutputValue::Text(text) => Some(text),
                ToolOutputValue::Image(_) => None,
                ToolOutputValue::Empty => None,
            })
            .collect()
    }

    fn contains(&self, needle: &str) -> bool {
        self.values.iter().any(|item| match item {
            ToolOutputValue::Text(text) => text.contains(needle),
            ToolOutputValue::Image(_) => false,
            ToolOutputValue::Empty => false,
        })
    }
}

pub trait AttachmentExtension {
    fn contains(&self, needle: &str) -> bool;
    fn as_image(&self) -> Option<&Image>;
}
impl AttachmentExtension for AttachmentContent {
    fn contains(&self, needle: &str) -> bool {
        match self {
            AttachmentContent::Image(_) => false,
            AttachmentContent::FileContent(content) => content.contains(needle),
        }
    }

    fn as_image(&self) -> Option<&Image> {
        match self {
            AttachmentContent::Image(image) => Some(image),
            AttachmentContent::FileContent(_) => None,
        }
    }
}
