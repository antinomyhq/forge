use crossterm::event::Event;
use derive_more::From;
use forge_api::{ChatResponse, ConversationId};

#[derive(From)]
pub enum Message {
    Chat(ChatResponse),
    ConversationId(ConversationId),
    KeyBoard(Event),
}
