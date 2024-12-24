use crate::completion::Completion;
use crate::conversation::Conversation;

// Shared state between each request to the server
pub struct App {
    pub completion: Completion,
    pub engine: Conversation,
}

impl App {
    pub fn new(working_dir: impl Into<String>) -> Self {
        let engine = Conversation;
        Self { completion: Completion::new(working_dir), engine }
    }
}
