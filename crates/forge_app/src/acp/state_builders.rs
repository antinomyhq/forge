//! State construction and MCP server management for ACP

use std::sync::Arc;

use agent_client_protocol as acp;
use forge_domain::{Agent, AgentId, Scope};

use crate::{
    AgentProviderResolver, AgentRegistry, ExternalMcpServer, McpImportService, McpService,
    ProviderAuthService, ProviderService, Services,
};

use super::conversion;
use super::error::{Error, Result};

/// Helper struct for building ACP state objects
pub(super) struct StateBuilders;

impl StateBuilders {
    /// Builds the SessionModeState from available agents
    pub(super) async fn build_session_mode_state<S: Services>(
        services: &S,
        current_agent_id: &AgentId,
    ) -> Result<acp::SessionModeState> {
        // Get all available agents from the registry
        let agents = services
            .agent_registry()
            .get_agents()
            .await
            .map_err(Error::Application)?;

        // Use conversion module to build the state
        Ok(conversion::build_session_mode_state(
            &agents,
            current_agent_id,
        ))
    }

    /// Builds the SessionModelState from available models for the agent's provider
    pub(super) async fn build_session_model_state<S: Services>(
        services: &S,
        current_agent: &Agent,
    ) -> Result<acp::SessionModelState> {
        // Resolve the provider for this agent
        let agent_provider_resolver = AgentProviderResolver::new(services.clone());
        let provider = agent_provider_resolver
            .get_provider(Some(current_agent.id.clone()))
            .await
            .map_err(Error::Application)?;

        // Refresh provider credentials
        let provider = services
            .provider_auth_service()
            .refresh_provider_credential(provider)
            .await
            .map_err(Error::Application)?;

        // Fetch models from the provider
        let mut models = services
            .provider_service()
            .models(provider)
            .await
            .map_err(Error::Application)?;
        models.sort_by(|a, b| a.name.cmp(&b.name));

        // Convert Forge models to ACP ModelInfo
        let available_models: Vec<acp::ModelInfo> = models
            .iter()
            .map(|model| {
                let mut model_info = acp::ModelInfo::new(
                    model.id.to_string(),
                    model.name.clone().unwrap_or_else(|| model.id.to_string()),
                )
                .description(model.description.clone());

                // Add metadata about model capabilities
                let mut meta = serde_json::Map::new();
                if let Some(context_length) = model.context_length {
                    meta.insert(
                        "contextLength".to_string(),
                        serde_json::json!(context_length),
                    );
                }
                if let Some(tools_supported) = model.tools_supported {
                    meta.insert(
                        "toolsSupported".to_string(),
                        serde_json::json!(tools_supported),
                    );
                }
                if let Some(supports_reasoning) = model.supports_reasoning {
                    meta.insert(
                        "supportsReasoning".to_string(),
                        serde_json::json!(supports_reasoning),
                    );
                }
                if !model.input_modalities.is_empty() {
                    let modalities: Vec<String> = model
                        .input_modalities
                        .iter()
                        .map(|m| format!("{:?}", m).to_lowercase())
                        .collect();
                    meta.insert("inputModalities".to_string(), serde_json::json!(modalities));
                }

                if !meta.is_empty() {
                    model_info = model_info.meta(meta);
                }

                model_info
            })
            .collect();

        Ok(
            acp::SessionModelState::new(current_agent.model.to_string(), available_models).meta({
                let mut meta = serde_json::Map::new();
                // Enable search functionality in the model dropdown
                meta.insert("searchable".to_string(), serde_json::json!(true));
                // Show search bar when there are more than 10 models
                meta.insert("searchThreshold".to_string(), serde_json::json!(10));
                // Enable filtering by model capabilities
                meta.insert("filterable".to_string(), serde_json::json!(true));
                // Suggest grouping models by provider
                meta.insert("groupBy".to_string(), serde_json::json!("provider"));
                meta
            }),
        )
    }

    /// Loads MCP servers from ACP requests into Forge's MCP configuration
    pub(super) async fn load_mcp_servers<S: Services>(
        services: &Arc<S>,
        mcp_servers: &[acp::McpServer],
    ) -> Result<()> {
        // Convert ACP MCP servers to ExternalMcpServer format
        let external_servers: Vec<ExternalMcpServer> = mcp_servers
            .iter()
            .map(Self::acp_to_external_mcp_server)
            .collect::<Result<Vec<_>>>()?;

        // Import via McpImportService
        (**services)
            .mcp_import_service()
            .import_servers(external_servers, &Scope::Local)
            .await
            .map_err(Error::Application)?;

        // Reload MCP servers to pick up the new configuration
        (**services)
            .mcp_service()
            .reload_mcp()
            .await
            .map_err(Error::Application)?;

        Ok(())
    }

    /// Converts an ACP McpServer to ExternalMcpServer format
    fn acp_to_external_mcp_server(server: &acp::McpServer) -> Result<ExternalMcpServer> {
        match server {
            acp::McpServer::Stdio(stdio) => {
                // Convert Vec<EnvVariable> to Vec<(String, String)>
                let env = stdio
                    .env
                    .iter()
                    .map(|e| (e.name.clone(), e.value.clone()))
                    .collect();

                Ok(ExternalMcpServer::Stdio {
                    name: stdio.name.clone(),
                    command: stdio.command.to_string_lossy().to_string(),
                    args: stdio.args.clone(),
                    env,
                })
            }
            acp::McpServer::Http(http) => {
                // Convert Vec<HttpHeader> to Vec<(String, String)>
                let headers = http
                    .headers
                    .iter()
                    .map(|h| (h.name.clone(), h.value.clone()))
                    .collect();

                Ok(ExternalMcpServer::Http {
                    name: http.name.clone(),
                    url: http.url.clone(),
                    headers,
                })
            }
            acp::McpServer::Sse(sse) => {
                // Convert Vec<HttpHeader> to Vec<(String, String)>
                let headers = sse
                    .headers
                    .iter()
                    .map(|h| (h.name.clone(), h.value.clone()))
                    .collect();

                Ok(ExternalMcpServer::Sse {
                    name: sse.name.clone(),
                    url: sse.url.clone(),
                    headers,
                })
            }
            _ => {
                // Handle future MCP server types that may be added to the protocol
                Err(Error::Application(anyhow::anyhow!(
                    "Unsupported MCP server type"
                )))
            }
        }
    }
}
