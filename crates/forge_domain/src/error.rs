use std::pin::Pin;

use thiserror::Error;

use crate::{AgentId, ConversationId};

// NOTE: Deriving From for error is a really bad idea. This is because you end
// up converting errors incorrectly without much context. For eg: You don't want
// all serde error to be treated as the same. Instead we want to know exactly
// where that serde failure happened and for what kind of value.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Missing tool name")]
    ToolCallMissingName,

    #[error("Invalid tool call arguments: {0}")]
    ToolCallArgument(serde_json::Error),

    #[error("Invalid tool call XML: {0}")]
    ToolCallParse(String),

    #[error("Invalid conversation id: {0}")]
    ConversationId(uuid::Error),

    #[error("Agent not found in the arena: {0}")]
    AgentUndefined(AgentId),

    #[error("Variable not found in output: {0}")]
    UndefinedVariable(String),

    #[error("Head agent not found")]
    HeadAgentUndefined,

    #[error("Agent '{0}' has reached max turns of {1}")]
    MaxTurnsReached(AgentId, u64),

    #[error("Conversation not found: {0}")]
    ConversationNotFound(ConversationId),

    #[error("Missing description for agent: {0}")]
    MissingAgentDescription(AgentId),
    #[error("Missing model for agent: {0}")]
    MissingModel(AgentId),
}

pub type Result<A> = std::result::Result<A, Error>;
pub type BoxStream<A, E> =
    Pin<Box<dyn tokio_stream::Stream<Item = std::result::Result<A, E>> + Send>>;

pub type ResultStream<A, E> = std::result::Result<BoxStream<A, E>, E>;
