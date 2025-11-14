use dashmap::DashMap;
use forge_app::domain::AgentId;
use forge_domain::Agent;

/// AgentRegistryService manages the active-agent ID and a registry of runtime
/// Agents in-memory.
pub struct ForgeAgentRegistryService {
    // In-memory storage for agents keyed by AgentId string
    agents: DashMap<String, Agent>,

    // In-memory storage for the active agent ID
    active_agent_id: tokio::sync::RwLock<Option<AgentId>>,
}

impl ForgeAgentRegistryService {
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            active_agent_id: tokio::sync::RwLock::new(None),
        }
    }
}

#[async_trait::async_trait]
impl forge_app::AgentRegistry for ForgeAgentRegistryService {
    async fn get_active_agent_id(&self) -> anyhow::Result<Option<AgentId>> {
        let agent_id = self.active_agent_id.read().await;
        Ok(agent_id.clone())
    }

    async fn set_active_agent_id(&self, agent_id: AgentId) -> anyhow::Result<()> {
        let mut active_agent = self.active_agent_id.write().await;
        *active_agent = Some(agent_id);
        Ok(())
    }

    async fn get_agents(&self) -> anyhow::Result<Vec<Agent>> {
        Ok(self
            .agents
            .iter()
            .map(|entry| entry.value().clone())
            .collect())
    }

    async fn get_agent(&self, agent_id: &AgentId) -> anyhow::Result<Option<Agent>> {
        Ok(self
            .agents
            .get(agent_id.as_str())
            .map(|v| v.value().clone()))
    }

    async fn set_agents(&self, agents: Vec<Agent>) -> anyhow::Result<()> {
        // Replace entire registry atomically: clear and insert
        self.agents.clear();
        for agent in agents {
            self.agents.insert(agent.id.as_str().to_string(), agent);
        }
        Ok(())
    }

    async fn insert_agent(&self, agent: Agent) -> anyhow::Result<()> {
        self.agents.insert(agent.id.as_str().to_string(), agent);
        Ok(())
    }
}
