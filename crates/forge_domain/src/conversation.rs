use derive_more::derive::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Context, Error, Metrics, Result, Workflow};

#[derive(Debug, Default, Display, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ConversationId(Uuid);

impl Copy for ConversationId {}

impl ConversationId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn into_string(&self) -> String {
        self.0.to_string()
    }

    pub fn parse(value: impl ToString) -> Result<Self> {
        Ok(Self(
            Uuid::parse_str(&value.to_string()).map_err(Error::ConversationId)?,
        ))
    }
}

#[derive(Debug, Setters, Serialize, Deserialize, Clone)]
#[setters(into, strip_option)]
pub struct Conversation {
    pub id: ConversationId,
    pub context: Option<Context>,
    // FIXME: add this to agent and drop from here
    // read from workflow and set in agent via prepare_agent function
    // in prepare_agent we should set the value from workflow only if its not already set
    pub max_tool_failure_per_turn: Option<usize>,
    // FIXME: add this to agent and drop from here
    // read from workflow and set in agent via prepare_agent function
    // in prepare_agent we should set the value from workflow only if its not already set
    pub max_requests_per_turn: Option<usize>,
    pub metrics: Metrics,
}

impl Conversation {
    pub fn reset_metric(&mut self) -> &mut Self {
        self.metrics = Metrics::new();
        self.metrics.start();
        self
    }

    pub fn new(id: ConversationId, workflow: Workflow) -> Self {
        let mut metrics = Metrics::new();
        metrics.start();

        Self {
            id,
            context: None,
            max_tool_failure_per_turn: workflow.max_tool_failure_per_turn,
            max_requests_per_turn: workflow.max_requests_per_turn,
            metrics,
        }
    }

    /// Generates an HTML representation of the conversation
    ///
    /// This method uses Handlebars to render the conversation as HTML
    /// from the template file, including all agents, events, and variables.
    ///
    /// # Errors
    /// - If the template file cannot be found or read
    /// - If the Handlebars template registration fails
    /// - If the template rendering fails
    pub fn to_html(&self) -> String {
        // Instead of using Handlebars, we now use our Element DSL
        crate::conversation_html::render_conversation_html(self)
    }
}
