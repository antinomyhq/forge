use std::fmt::Display;

use derive_setters::Setters;
use forge_provider::{AnyMessage, Message};
use serde::Serialize;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Setters)]
#[setters(into)]
pub struct Tag {
    // TODO: move to enum type
    pub name: String,
    pub attributes: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum MessageTemplate {
    Tagged {
        tag: Tag,
        content: String,
    },
    Combine {
        left: Box<MessageTemplate>,
        right: Box<MessageTemplate>,
    },
}

impl MessageTemplate {
    pub fn new(tag: Tag, content: impl ToString) -> Self {
        Self::Tagged { tag, content: content.to_string() }
    }

    pub fn task<T: ToString>(content: T) -> Self {
        let tag = Tag { name: "task".to_string(), attributes: vec![] };

        Self::new(tag, content.to_string())
    }

    pub fn file<S: ToString, T: ToString>(path: S, content: T) -> Self {
        let tag = Tag {
            name: "file_content".to_string(),
            attributes: vec![("path".to_string(), path.to_string())],
        };

        Self::new(tag, content.to_string())
    }

    pub fn append(self, other: Self) -> Self {
        Self::Combine { left: Box::new(self), right: Box::new(other) }
    }
}

impl Display for MessageTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageTemplate::Tagged { tag, content } => {
                let tag_name = tag.name.to_uppercase();
                f.write_str("<")?;
                f.write_str(&tag_name)?;

                for (key, value) in &tag.attributes {
                    f.write_str(" ")?;
                    f.write_str(key)?;
                    f.write_str("=\"")?;
                    f.write_str(value)?;
                    f.write_str("\"")?;
                }

                f.write_str(">")?;

                f.write_str(content)?;

                f.write_str("<")?;
                f.write_str(&tag_name)?;
                f.write_str("/>")?;
            }
            MessageTemplate::Combine { left, right } => {
                write!(f, "{}\n{}", left, right)?;
            }
        }

        Ok(())
    }
}

impl From<MessageTemplate> for AnyMessage {
    fn from(value: MessageTemplate) -> Self {
        Message::user(value.to_string()).into()
    }
}
