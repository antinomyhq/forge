use forge_api::ChatRequest;

#[derive(Default)]
pub struct CommandList {
    commands: Vec<Command>,
}

impl CommandList {
    pub fn insert(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn into_inner(self) -> Vec<Command> {
        self.commands
    }
}

#[derive(Debug)]
pub enum Command {
    Suspend,
    ToggleMode(String),
    UserMessage(ChatRequest),
    InitConversation,
    Exit,
}
