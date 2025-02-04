use std::collections::HashMap;
use std::path::PathBuf;

use derive_more::derive::Display;
use derive_setters::Setters;
use schemars::schema::RootSchema;
use serde::Serialize;

use crate::{Environment, ModelId, Provider, ToolName};

#[derive(Default, Serialize)]
pub struct Variables(HashMap<String, String>);
impl Variables {
    pub fn add(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.0.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.0.get(key)
    }
}

#[derive(Serialize, Setters, Clone)]
pub struct SystemContext {
    pub env: Environment,
    pub tool_information: String,
    pub tool_supported: bool,
    pub custom_instructions: Option<String>,
    pub files: Vec<String>,
}

pub enum PromptContent {
    Text(String),
    File(PathBuf),
}

pub struct Prompt<V> {
    pub template: PromptTemplate,
    pub variables: Schema<V>,
}

impl<V> Prompt<V> {
    pub fn render(&self, _variables: &V) -> String {
        todo!()
    }
}

pub struct Schema<S> {
    pub schema: RootSchema,
    _marker: std::marker::PhantomData<S>,
}

pub enum PromptTemplate {
    File(PathBuf),
    Literal(String),
}

#[derive(Debug, Display, Eq, PartialEq, Hash, Clone)]
pub struct AgentId(String);

pub struct Agent {
    pub id: AgentId,
    pub provider: Provider,
    pub model: ModelId,
    pub description: String,
    pub system_prompt: Prompt<SystemContext>,
    pub user_prompt: Prompt<Variables>,
    pub tools: Vec<ToolName>,
    pub transforms: Vec<Transform>,
}

/// Possible use cases for transforms:
/// - Summarization (TokenLimit)
///   - Remove all, except first, and add summary as an assistant message
/// - Enhance user prompt
///   - Add additional meta information to the last user prompt
/// - Standard middle-out implementation like in Open Router NOTE: The
///   transforms are applied in the order they are defined (0th to last)
pub enum Transform {
    Summarize {
        input: String,
        agent_id: AgentId,
        token_limit: usize,
    },

    EnhanceUserPrompt {
        agent_id: AgentId,
        input: String,
    },
}

impl Agent {
    pub fn new(_name: impl Into<String>) -> Self {
        todo!()
    }
}
