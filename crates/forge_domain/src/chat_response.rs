use std::time::Duration;

use crate::{Metrics, ToolCallFull, ToolResult, Usage};

/// Events that are emitted by the agent for external consumption. This includes
/// events for all internal state changes.
#[derive(Debug, Clone)]
pub enum ChatResponse {
    Text {
        text: String,
        is_complete: bool,
        is_md: bool,
    },
    Summary {
        content: String,
    },
    ToolCallStart(ToolCallFull),
    ToolCallEnd(ToolResult),
    Usage(Usage),
    RetryAttempt {
        cause: Cause,
        duration: Duration,
    },
    Interrupt {
        reason: InterruptionReason,
    },
    Reasoning {
        content: String,
    },
    ChatComplete(Metrics),
}

#[derive(Debug, Clone)]
pub enum InterruptionReason {
    MaxToolFailurePerTurnLimitReached { limit: u64 },
    MaxRequestPerTurnLimitReached { limit: u64 },
}

#[derive(Clone)]
pub struct Cause(String);

impl Cause {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl From<&anyhow::Error> for Cause {
    fn from(value: &anyhow::Error) -> Self {
        Self(format!("{value:?}"))
    }
}
