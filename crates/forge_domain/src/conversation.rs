use std::collections::{HashMap, VecDeque};

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
    pub events: Vec<Event>,
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
            events: Default::default(),
        }
    }

    pub fn turn_count(&self, id: &AgentId) -> Option<u64> {
        self.state.get(id).map(|s| s.turn_count)
    }

    /// Returns all the agents that are subscribed to the given event.
    pub fn subscriptions(&self, event_name: &str) -> Vec<Agent> {
        self.workflow
            .agents
            .iter()
            .filter(|a| {
                // Filter out disabled agents
                !a.disable.unwrap_or_default()
            })
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
    pub fn insert_event(&mut self, event: Event) -> &mut Self {
        let subscribed_agents = self.subscriptions(&event.name);
        self.events.push(event.clone());

        subscribed_agents.iter().for_each(|agent| {
            self.state
                .entry(agent.id.clone())
                .or_default()
                .queue
                .push_back(event.clone());
        });

        self
    }

    /// Gets the next event for a specific agent, if one is available
    ///
    /// If an event is available in the agent's queue, it is popped and
    /// returned. Additionally, if the agent's queue becomes empty, it is
    /// marked as inactive.
    ///
    /// Returns None if no events are available for this agent.
    pub fn poll_event(&mut self, agent_id: &AgentId) -> Option<Event> {
        // if event is present in queue, pop it and return.
        if let Some(agent) = self.state.get_mut(agent_id) {
            if let Some(event) = agent.queue.pop_front() {
                return Some(event);
            }
        }
        None
    }

    /// Dispatches an event to all subscribed agents and activates any inactive
    /// agents
    ///
    /// This method performs two main operations:
    /// 1. Adds the event to the queue of all agents that subscribe to this
    ///    event type
    /// 2. Activates any inactive agents (where is_active=false) that are
    ///    subscribed to the event
    ///
    /// Returns a vector of AgentIds for all agents that were inactive and are
    /// now activated
    pub fn dispatch_event(&mut self, event: Event) -> Vec<AgentId> {
        let name = event.name.as_str();
        let mut agents = self.subscriptions(name);

        let inactive_agents = agents
            .iter_mut()
            .filter_map(|agent| {
                let is_inactive = self
                    .state
                    .get(&agent.id)
                    .map(|state| state.queue.is_empty())
                    .unwrap_or(true);
                if is_inactive {
                    Some(agent.id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        self.insert_event(event);

        inactive_agents
    }
}
