use std::fmt::Display;

use forge_provider::{AnyMessage, Message};

pub struct Tag {
    // TODO: move to enum type
    name: String,
    attributes: Vec<(String, String)>,
}

pub enum PromptTemplate {
    Tagged {
        tag: Tag,
        content: String,
    },
    Combine {
        left: Box<PromptTemplate>,
        right: Box<PromptTemplate>,
    },
}

impl PromptTemplate {
    fn new(tag: Tag, content: String) -> Self {
        Self::Tagged { tag, content }
    }

    pub fn task(content: String) -> Self {
        let tag = Tag { name: "task".to_string(), attributes: vec![] };

        Self::new(tag, content)
    }

    pub fn file(path: String, content: String) -> Self {
        let tag = Tag {
            name: "file_content".to_string(),
            attributes: vec![("path".to_string(), path)],
        };

        Self::new(tag, content)
    }

    pub fn append(self, other: Self) -> Self {
        Self::Combine { left: Box::new(self), right: Box::new(other) }
    }
}

impl Display for PromptTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptTemplate::Tagged { tag, content } => {
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
            PromptTemplate::Combine { left, right } => {
                write!(f, "{}\n{}", left, right)?;
            }
        }

        Ok(())
    }
}

impl From<PromptTemplate> for AnyMessage {
    fn from(value: PromptTemplate) -> Self {
        Message::user(value.to_string()).into()
    }
}
