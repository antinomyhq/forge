#[derive(Clone, Default)]
pub struct Workspace {
    pub current_branch: Option<String>,
    pub current_dir: Option<String>,
}
