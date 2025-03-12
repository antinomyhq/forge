use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;
use derive_more::derive::Display;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{Agent, AgentId, Context, Error, Event, Workflow};

#[derive(Debug, Display, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ConversationId(Uuid);

impl ConversationId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn into_string(&self) -> String {
        self.0.to_string()
    }

    pub fn parse(value: impl ToString) -> Result<Self, Error> {
        Ok(Self(
            Uuid::parse_str(&value.to_string()).map_err(Error::ConversationId)?,
        ))
    }
}

#[derive(Debug, Setters, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: ConversationId,
    pub archived: bool,
    pub state: HashMap<AgentId, AgentState>,
    pub workflow: Workflow,
    pub variables: HashMap<String, Value>,
    pub active_agents: HashSet<AgentId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentState {
    pub turn_count: u64,
    pub context: Option<Context>,
    /// holds the events that are waiting to be processed
    pub queue: VecDeque<Event>,
}

impl Conversation {
    pub fn new(id: ConversationId, workflow: Workflow) -> Self {
        Self {
            id,
            archived: false,
            state: Default::default(),
            variables: workflow.variables.clone().unwrap_or_default(),
            workflow,
            active_agents: HashSet::new(),
        }
    }

    pub fn turn_count(&self, id: &AgentId) -> Option<u64> {
        self.state.get(id).map(|s| s.turn_count)
    }

    /// Returns all the agents that are subscribed to the given event.
    pub fn entries(&self, event_name: &str) -> Vec<Agent> {
        self.workflow
            .agents
            .iter()
            .filter(|a| {
                self.turn_count(&a.id).unwrap_or_default() < a.max_turns.unwrap_or(u64::MAX)
            })
            .filter(|a| {
                a.subscribe
                    .as_ref()
                    .is_some_and(|subs| subs.contains(&event_name.to_string()))
            })
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn context(&self, id: &AgentId) -> Option<&Context> {
        self.state.get(id).and_then(|s| s.context.as_ref())
    }

    pub fn rfind_event(&self, event_name: &str) -> Option<&Event> {
        self.state
            .values()
            .flat_map(|state| state.queue.iter().rev())
            .find(|event| event.name == event_name)
    }

    /// Get a variable value by its key
    ///
    /// Returns None if the variable doesn't exist
    pub fn get_variable(&self, key: &str) -> Option<&Value> {
        self.variables.get(key)
    }

    /// Set a variable with the given key and value
    ///
    /// If the key already exists, its value will be updated
    pub fn set_variable(&mut self, key: String, value: Value) -> &mut Self {
        self.variables.insert(key, value);
        self
    }

    /// Delete a variable by its key
    ///
    /// Returns true if the variable was present and removed, false otherwise
    pub fn delete_variable(&mut self, key: &str) -> bool {
        self.variables.remove(key).is_some()
    }

    /// Add an event to the queue of subscribed agents
    pub fn add_event(&mut self, event: Event) -> &mut Self {
        let subscribed_agents = self.entries(&event.name);

        subscribed_agents.iter().for_each(|agent| {
            self.state
                .entry(agent.id.clone())
                .or_default()
                .queue
                .push_back(event.clone());
        });

        self
    }

    /// Check if an agent is currently active
    pub fn is_agent_active(&self, agent_id: &AgentId) -> bool {
        self.active_agents.contains(agent_id)
    }
}
