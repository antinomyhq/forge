use std::path::PathBuf;

use derive_more::derive::Display;
use derive_setters::Setters;
use handlebars::Handlebars;
use schemars::schema::RootSchema;
use serde::Serialize;

use crate::variables::Variables;
use crate::{Environment, Error, ModelId, Provider, ToolName};

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

impl<V: Serialize> Prompt<V> {
    pub fn render(&self, ctx: &V) -> crate::Result<String> {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        hb.register_escape_fn(|str| str.to_string());

        hb.render_template(self.template.as_str(), &ctx)
            .map_err(Error::Template)
    }
}

#[derive(Debug, Clone)]
pub struct Schema<S> {
    pub schema: RootSchema,
    _marker: std::marker::PhantomData<S>,
}

pub struct PromptTemplate(String);
impl PromptTemplate {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
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

/// Transformations that can be applied to the agent's context before sending it
/// upstream to the provider.
pub enum Transform {
    /// Compresses multiple assistant messages into a single message
    Assistant {
        input: String,
        output: String,
        agent_id: AgentId,
        token_limit: usize,
    },

    /// Works on the user prompt by enriching it with additional information
    User {
        agent_id: AgentId,
        input: String,
        output: String,
    },

    /// Intercepts the context and performs an operation without changing the
    /// context
    Tap { agent_id: AgentId, input: String },
}

impl Agent {
    pub fn new(_name: impl Into<String>) -> Self {
        todo!()
    }
}
