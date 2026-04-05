use std::collections::BTreeMap;
use std::sync::Arc;

use agent_client_protocol as acp;
use forge_domain::{Agent, AgentId, McpHttpServer, McpServerConfig, Scope, ServerName};

use crate::{
    AgentProviderResolver, AgentRegistry, McpConfigManager, McpService, ProviderAuthService,
    ProviderService, Services,
};

use super::conversion;
use super::error::{Error, Result};

pub(super) struct StateBuilders;

impl StateBuilders {
    pub(super) async fn build_session_mode_state<S: Services + ?Sized>(
        services: &S,
        current_agent_id: &AgentId,
    ) -> Result<acp::SessionModeState> {
        let agents = services
            .agent_registry()
            .get_agents()
            .await
            .map_err(Error::Application)?;

        Ok(conversion::build_session_mode_state(
            &agents,
            current_agent_id,
        ))
    }

    pub(super) async fn build_session_model_state<S: Services>(
        services: &Arc<S>,
        current_agent: &Agent,
    ) -> Result<acp::SessionModelState> {
        let agent_provider_resolver = AgentProviderResolver::new(services.clone());
        let provider = agent_provider_resolver
            .get_provider(Some(current_agent.id.clone()))
            .await
            .map_err(Error::Application)?;
        let provider = services
            .provider_auth_service()
            .refresh_provider_credential(provider)
            .await
            .map_err(Error::Application)?;

        let mut models = services
            .provider_service()
            .models(provider)
            .await
            .map_err(Error::Application)?;
        models.sort_by(|left, right| left.name.cmp(&right.name));

        let available_models = models
            .iter()
            .map(|model| {
                let mut model_info = acp::ModelInfo::new(
                    model.id.to_string(),
                    model.name.clone().unwrap_or_else(|| model.id.to_string()),
                )
                .description(model.description.clone());

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
                    let modalities = model
                        .input_modalities
                        .iter()
                        .map(|modality| format!("{:?}", modality).to_lowercase())
                        .collect::<Vec<_>>();
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
                meta.insert("searchable".to_string(), serde_json::json!(true));
                meta.insert("searchThreshold".to_string(), serde_json::json!(10));
                meta.insert("filterable".to_string(), serde_json::json!(true));
                meta.insert("groupBy".to_string(), serde_json::json!("provider"));
                meta
            }),
        )
    }

    pub(super) async fn load_mcp_servers<S: Services + ?Sized>(
        services: &S,
        mcp_servers: &[acp::McpServer],
    ) -> Result<()> {
        let mut config = services
            .mcp_config_manager()
            .read_mcp_config(Some(&Scope::Local))
            .await
            .map_err(Error::Application)?;

        for server in mcp_servers {
            let (name, server_config) = Self::acp_to_mcp_server_config(server)?;
            config.mcp_servers.insert(name, server_config);
        }

        services
            .mcp_config_manager()
            .write_mcp_config(&config, &Scope::Local)
            .await
            .map_err(Error::Application)?;
        services.mcp_service().reload_mcp().await.map_err(Error::Application)?;
        Ok(())
    }

    fn acp_to_mcp_server_config(server: &acp::McpServer) -> Result<(ServerName, McpServerConfig)> {
        match server {
            acp::McpServer::Stdio(stdio) => {
                let env = stdio
                    .env
                    .iter()
                    .map(|entry| (entry.name.clone(), entry.value.clone()))
                    .collect::<BTreeMap<_, _>>();
                Ok((
                    ServerName::from(stdio.name.clone()),
                    McpServerConfig::new_stdio(stdio.command.to_string_lossy().to_string(), stdio.args.clone(), Some(env)),
                ))
            }
            acp::McpServer::Http(http) => Ok((
                ServerName::from(http.name.clone()),
                McpServerConfig::Http(McpHttpServer {
                    url: http.url.clone(),
                    headers: http
                        .headers
                        .iter()
                        .map(|header| (header.name.clone(), header.value.clone()))
                        .collect(),
                    timeout: None,
                    disable: false,
                }),
            )),
            acp::McpServer::Sse(sse) => Ok((
                ServerName::from(sse.name.clone()),
                McpServerConfig::Http(McpHttpServer {
                    url: sse.url.clone(),
                    headers: sse
                        .headers
                        .iter()
                        .map(|header| (header.name.clone(), header.value.clone()))
                        .collect(),
                    timeout: None,
                    disable: false,
                }),
            )),
            _ => Err(Error::Application(anyhow::anyhow!(
                "Unsupported MCP server type"
            ))),
        }
    }
}