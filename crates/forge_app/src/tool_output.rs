use std::path::PathBuf;
use forge_domain::Image;

pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
}

pub struct PatchOutput {
    pub before: String,
    pub after: String,
}

pub struct ReadOutput {
    pub content: Content,
}

pub enum Content {
    File(String),
    Image(Image),
}

pub struct SearchResult {
    pub line: String,
    pub matches: Vec<String>,
    pub path: Option<PathBuf>,
}

pub struct FetchOutput {
    pub content: String,
    pub code: u16,
}

