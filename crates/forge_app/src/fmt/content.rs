use forge_domain::{ChatResponseContent, Config};

pub trait FormatContent {
    fn to_content(&self, env: &Config) -> Option<ChatResponseContent>;
}
