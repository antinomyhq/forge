use forge_display::TitleFormat;
use forge_domain::{ChatResponse, Environment};

#[derive(Debug, PartialEq)]
pub enum ContentFormat {
    Title(TitleFormat),
    PlainText(String),
    Markdown(String),
}

impl From<ContentFormat> for ChatResponse {
    fn from(value: ContentFormat) -> Self {
        match value {
            ContentFormat::Title(title) => {
                ChatResponse::TaskMessage { text: title.to_string(), is_md: false }
            }
            ContentFormat::PlainText(text) => ChatResponse::TaskMessage { text, is_md: false },
            ContentFormat::Markdown(text) => ChatResponse::TaskMessage { text, is_md: true },
        }
    }
}

impl From<TitleFormat> for ContentFormat {
    fn from(title: TitleFormat) -> Self {
        ContentFormat::Title(title)
    }
}

pub trait FormatContent {
    fn to_content(&self, env: &Environment) -> Option<ContentFormat>;
}
