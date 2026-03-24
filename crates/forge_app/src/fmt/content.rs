use forge_domain::ChatResponseContent;
use forge_env::Environment;

pub trait FormatContent {
    fn to_content(&self, env: &Environment) -> Option<ChatResponseContent>;
}
