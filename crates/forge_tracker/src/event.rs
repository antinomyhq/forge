use std::ops::Deref;

use chrono::{DateTime, Utc};
use convert_case::{Case, Casing};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub event_name: Name,
    pub event_value: String,
    pub start_time: DateTime<Utc>,
    pub cores: usize,
    pub client_id: String,
    pub os_name: String,
    pub up_time: i64,
    pub path: Option<String>,
    pub cwd: Option<String>,
    pub user: String,
    pub args: Vec<String>,
    pub version: String,
    pub email: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Name(String);
impl From<String> for Name {
    fn from(name: String) -> Self {
        Self(name.to_case(Case::Snake))
    }
}
impl Deref for Name {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Name> for String {
    fn from(val: Name) -> Self {
        val.0
    }
}

#[derive(Debug, Clone)]
pub enum EventKind {
    Start,
    Ping,
    Prompt(String),
    Error(String),
    ErrorOccurred(ErrorDetails),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub error_type: String,
    pub error_message: String,
    pub context: String,
    pub stack_trace: String,
}

impl EventKind {
    pub fn name(&self) -> Name {
        match self {
            Self::Start => Name::from("start".to_string()),
            Self::Ping => Name::from("ping".to_string()),
            Self::Prompt(_) => Name::from("prompt".to_string()),
            Self::Error(_) => Name::from("error".to_string()),
            Self::ErrorOccurred(_) => Name::from("error_occured".to_string()),
        }
    }
    pub fn value(&self) -> String {
        match self {
            Self::Start => "".to_string(),
            Self::Ping => "".to_string(),
            Self::Prompt(content) => content.to_string(),
            Self::Error(content) => content.to_string(),
            Self::ErrorOccurred(details) => serde_json::to_string(details).unwrap_or_default(),
        }
    }
}
