use serde::{Deserialize, Serialize};

use crate::{AgentId, ProviderId};

/// Represents the scope at which a configuration value should be resolved or
/// set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConfigScope {
    Global,
    Project,
    Provider(ProviderId),
    Agent(AgentId),
    Or(Box<ConfigScope>, Box<ConfigScope>),
}
