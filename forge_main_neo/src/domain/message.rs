use crossterm::event::Event;
use derive_more::From;
use forge_api::{AgentMessage, ChatResponse, ConversationId};

#[derive(From)]
pub enum Message {
    Chat(AgentMessage<ChatResponse>),
    ConversationId(ConversationId),
    KeyBoard(Event),
}
