use forge_domain::{ChatResponseContent, Environment};

pub trait FormatContent {
    fn to_content(&self, env: &Environment) -> Option<ChatResponseContent>;
}
