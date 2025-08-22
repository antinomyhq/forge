use forge_domain::{ChatResponseContent, Environment, TitleFormat};

pub fn title_to_content_format(title: TitleFormat) -> ChatResponseContent {
    ChatResponseContent::Title(title.to_string())
}

pub trait FormatContent {
    fn to_content(&self, env: &Environment) -> Option<ChatResponseContent>;
}
