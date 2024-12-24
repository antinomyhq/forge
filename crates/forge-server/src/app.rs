use crate::completion::Completion;

// Shared state between each request to the server
pub struct App {
    completion: Completion,
}

impl App {
    pub fn new(path: impl Into<String>) -> Self {
        Self { completion: Completion::new(path) }
    }
}
