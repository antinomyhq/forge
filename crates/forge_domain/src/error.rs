use std::pin::Pin;

use derive_more::From;
use forge_json_repair::JsonRepairError;
use thiserror::Error;

use crate::{AgentId, ConversationId};

// NOTE: Deriving From for error is a really bad idea. This is because you end
// up converting errors incorrectly without much context. For eg: You don't want
// all serde error to be treated as the same. Instead we want to know exactly
// where that serde failure happened and for what kind of value.
#[derive(Debug, Error, From)]
pub enum Error {
    #[error("Missing tool name")]
    ToolCallMissingName,

    #[error("{0}")]
    EToolCallArgument(ToolCallArgumentError),

    #[error("JSON deserialization error: {error}")]
    #[from(skip)]
    ToolCallArgument {
        error: JsonRepairError,
        args: String,
    },

    #[error("Invalid tool call XML: {0}")]
    #[from(skip)]
    ToolCallParse(String),

    #[error("Invalid conversation id: {0}")]
    ConversationId(uuid::Error),

    #[error("Agent not found in the arena: {0}")]
    AgentUndefined(AgentId),

    #[error("Variable not found in output: {0}")]
    #[from(skip)]
    UndefinedVariable(String),

    #[error("Head agent not found")]
    HeadAgentUndefined,

    #[error("Agent '{0}' has reached max turns of {1}")]
    MaxTurnsReached(AgentId, u64),

    #[error("Conversation not found: {0}")]
    ConversationNotFound(ConversationId),

    #[error("Missing description for agent: {0}")]
    #[from(skip)]
    MissingAgentDescription(AgentId),

    #[error("Missing model for agent: {0}")]
    #[from(skip)]
    MissingModel(AgentId),

    #[error("No model defined for agent: {0}")]
    #[from(skip)]
    NoModelDefined(AgentId),

    #[error(transparent)]
    Retryable(anyhow::Error),
}

pub type Result<A> = std::result::Result<A, Error>;
pub type BoxStream<A, E> =
    Pin<Box<dyn tokio_stream::Stream<Item = std::result::Result<A, E>> + Send>>;

pub type ResultStream<A, E> = std::result::Result<BoxStream<A, E>, E>;

#[derive(Debug, derive_more::From)]
pub struct ToolCallArgumentError(eserde::DeserializationErrors);

impl std::error::Error for ToolCallArgumentError {}

impl std::fmt::Display for ToolCallArgumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Invalid tool call arguments:")?;
        for error in self.0.iter() {
            writeln!(f, "- {error}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use forge_json_repair::JsonRepairError;
    use serde_json::Value;

    use crate::Error;

    #[test]
    fn test_debug_serde_error() {
        let args = "{a: 1}";
        let serde_error = serde_json::from_str::<Value>(&args).unwrap_err();
        let a = Error::ToolCallArgument {
            error: JsonRepairError::from(serde_error),
            args: args.to_string(),
        };
        let a = anyhow::anyhow!(a);
        eprintln!("{:?}", a.root_cause());
    }
}
