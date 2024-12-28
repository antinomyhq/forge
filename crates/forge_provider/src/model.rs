use derive_more::derive::From;
use derive_setters::Setters;
use forge_tool::{Tool, ToolName};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

#[derive(Default, Setters, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub messages: Vec<AnyMessage>,
    pub tools: Vec<Tool>,
    pub tool_result: Vec<ToolResult>,
    pub model: ModelId,
}

impl Request {
    pub fn new(id: ModelId) -> Self {
        Request {
            messages: vec![],
            tools: vec![],
            tool_result: vec![],
            model: id,
        }
    }

    pub fn add_tool(mut self, tool: impl Into<Tool>) -> Self {
        self.add_tool_mut(tool);
        self
    }

    pub fn add_tool_result(mut self, tool_result: impl Into<ToolResult>) -> Self {
        self.add_tool_result_mut(tool_result);
        self
    }

    pub fn add_message(mut self, message: impl Into<AnyMessage>) -> Self {
        self.add_message_mut(message);
        self
    }

    pub fn extend_tools(mut self, tools: Vec<impl Into<Tool>>) -> Self {
        self.extend_tools_mut(tools);
        self
    }

    pub fn extend_tool_results(mut self, tool_results: Vec<impl Into<ToolResult>>) -> Self {
        self.extend_tool_results_mut(tool_results);
        self
    }

    pub fn extend_messages(mut self, messages: Vec<impl Into<AnyMessage>>) -> Self {
        self.extend_messages_mut(messages);
        self
    }

    pub fn add_tool_mut(&mut self, tool: impl Into<Tool>) {
        let tool: Tool = tool.into();
        self.tools.push(tool);
    }

    pub fn add_tool_result_mut(&mut self, tool_result: impl Into<ToolResult>) {
        self.tool_result.push(tool_result.into());
    }

    pub fn add_message_mut(&mut self, message: impl Into<AnyMessage>) {
        self.messages.push(message.into());
    }

    pub fn extend_tools_mut(&mut self, tools: Vec<impl Into<Tool>>) {
        self.tools.extend(tools.into_iter().map(Into::into));
    }

    pub fn extend_tool_results_mut(&mut self, tool_results: Vec<impl Into<ToolResult>>) {
        self.tool_result
            .extend(tool_results.into_iter().map(Into::into));
    }

    pub fn extend_messages_mut(&mut self, messages: Vec<impl Into<AnyMessage>>) {
        self.messages.extend(messages.into_iter().map(Into::into));
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct System;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User;

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Assistant;

pub trait Role {
    fn name() -> String;
}

impl Role for System {
    fn name() -> String {
        "system".to_string()
    }
}

impl Role for User {
    fn name() -> String {
        "user".to_string()
    }
}

impl Role for Assistant {
    fn name() -> String {
        "assistant".to_string()
    }
}

#[derive(Default, Setters, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message<R: Role> {
    pub content: String,
    #[serde(skip)]
    _r: std::marker::PhantomData<R>,
}

impl<T: Role> Message<T> {
    pub fn extend(self, other: Message<T>) -> Message<T> {
        Message {
            content: format!("{}\n{}", self.content, other.content),
            _r: Default::default(),
        }
    }
}

impl Message<System> {
    pub fn system(content: impl Into<String>) -> Self {
        Message { content: content.into(), _r: Default::default() }
    }
}

impl Message<User> {
    pub fn user(content: impl Into<String>) -> Self {
        Message { content: content.into(), _r: Default::default() }
    }

    /// Creates a user message from any serializable item. The message is
    /// typically in a XML format
    pub fn try_from(item: impl Serialize) -> Result<Self> {
        Ok(Message::user(serde_json::to_string(&item)?))
    }
}

impl Message<Assistant> {
    pub fn assistant(content: impl Into<String>) -> Self {
        Message { content: content.into(), _r: Default::default() }
    }
}

#[derive(Debug, Clone, From, Serialize, Deserialize, PartialEq)]
pub enum AnyMessage {
    System(Message<System>),
    User(Message<User>),
    Assistant(Message<Assistant>),
}

impl AnyMessage {
    pub fn content(&self) -> &str {
        match self {
            AnyMessage::System(msg) => msg.content.as_str(),
            AnyMessage::User(msg) => msg.content.as_str(),
            AnyMessage::Assistant(msg) => msg.content.as_str(),
        }
    }
}

#[derive(Default, Setters, Debug, Clone)]
#[setters(into)]
pub struct Response {
    pub message: Message<Assistant>,
    pub tool_use: Vec<ToolUsePart>,
    pub finish_reason: Option<FinishReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinishReason {
    ToolUse,
    EndTurn,
}

impl FinishReason {
    pub fn parse(reason: String) -> Option<Self> {
        match reason.as_str() {
            "tool_use" => Some(FinishReason::ToolUse),
            "tool_calls" => Some(FinishReason::ToolUse),
            "end_turn" => Some(FinishReason::EndTurn),
            _ => None,
        }
    }
}

impl Response {
    pub fn new(message: String) -> Response {
        Response {
            message: Message::assistant(message),
            tool_use: vec![],
            finish_reason: None,
        }
    }

    pub fn add_call(mut self, call_tool: impl Into<ToolUsePart>) -> Self {
        self.tool_use.push(call_tool.into());
        self
    }

    pub fn extend_calls(mut self, calls: Vec<impl Into<ToolUsePart>>) -> Self {
        self.tool_use.extend(calls.into_iter().map(Into::into));
        self
    }
}

/// Unique identifier for a using a tool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UseId(pub(crate) String);

impl<A: ToString> From<A> for UseId {
    fn from(value: A) -> Self {
        UseId(value.to_string())
    }
}

/// Contains a part message for using a tool. This is received as a part of the
/// response from the model only when streaming is enabled.
#[derive(Default, Setters, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[setters(strip_option, into)]
pub struct ToolUsePart {
    /// Optional unique identifier that represents a single call to the tool
    /// use. NOTE: Not all models support a call ID for using a tool
    pub use_id: Option<UseId>,
    pub name: Option<ToolName>,

    /// Arguments that need to be passed to the tool. NOTE: Not all tools
    /// require input
    pub argument_part: String,
}

/// Contains the full information about using a tool. This is received as a part
/// of the response from the model when streaming is disabled.
#[derive(Default, Setters, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolUse {
    pub name: ToolName,
    pub use_id: Option<UseId>,
    pub arguments: Value,
}

impl ToolUse {
    pub fn try_from_parts(parts: Vec<ToolUsePart>) -> Result<Self> {
        let mut tool_name = None;
        let mut tool_use_id = None;

        let mut input = String::new();
        for part in parts {
            if let Some(value) = part.name {
                tool_name = Some(value);
            }

            if let Some(value) = part.use_id {
                tool_use_id = Some(value);
            }

            input.push_str(&part.argument_part);
        }

        if let Some(tool_name) = tool_name {
            Ok(ToolUse {
                name: tool_name,
                use_id: tool_use_id,
                arguments: serde_json::from_str(&input)?,
            })
        } else {
            Err(Error::ToolUserMissingName)
        }
    }
}

#[derive(Default, Setters, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolResult {
    pub tool_name: ToolName,
    pub tool_use_id: Option<UseId>,
    pub content: Value,
    pub is_error: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Setters)]
pub struct Model {
    pub id: ModelId,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(transparent)]
pub struct ModelId(String);

impl Default for ModelId {
    fn default() -> Self {
        ModelId("openai/gpt-3.5-turbo".to_string())
    }
}
