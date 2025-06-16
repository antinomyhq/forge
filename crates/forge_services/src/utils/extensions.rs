use forge_domain::{AttachmentContent, Image};

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
