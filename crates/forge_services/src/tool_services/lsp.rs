use std::sync::Arc;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use forge_app::LspService;
use forge_domain::{LspTool, ToolOutput, LspOperation};
use forge_lsp::LspManager;

use crate::utils::assert_absolute_path;

pub struct ForgeLspService<I> {
    #[allow(dead_code)]
    infra: Arc<I>,
    manager: Arc<LspManager>,
}

impl<I> ForgeLspService<I> {
    pub fn new(infra: Arc<I>) -> Self {
        Self {
            infra,
            manager: Arc::new(LspManager::new()),
        }
    }
}

#[async_trait::async_trait]
impl<I: Send + Sync> LspService for ForgeLspService<I> {
    async fn execute_lsp(&self, tool: LspTool) -> Result<ToolOutput> {
        let path = PathBuf::from(&tool.file_path);
        assert_absolute_path(&path)?;

        if !path.exists() && !matches!(tool.operation, LspOperation::WorkspaceSymbol) {
             return Err(anyhow!("File not found: {}", tool.file_path));
        }

        let client = self.manager.get_client(&path).await?;

        // Ensure file is open
        if !matches!(tool.operation, LspOperation::WorkspaceSymbol) {
            let content = std::fs::read_to_string(&path)?;
            let language_id = forge_lsp::language::get_language_id(&path).unwrap_or("plaintext");
            client.did_open(&path, language_id, content).await?;
        }

        let line = tool.line.unwrap_or(1).saturating_sub(1); // 1-based to 0-based
        let character = tool.character.unwrap_or(1).saturating_sub(1);

        match tool.operation {
            LspOperation::GoToDefinition => {
                let result = client.goto_definition(&path, line, character).await?;
                let output = serde_json::to_string_pretty(&result)?;
                Ok(ToolOutput::text(output))
            }
            LspOperation::FindReferences => {
                let result = client.find_references(&path, line, character).await?;
                let output = serde_json::to_string_pretty(&result)?;
                Ok(ToolOutput::text(output))
            }
            LspOperation::Hover => {
                let result = client.hover(&path, line, character).await?;
                let output = serde_json::to_string_pretty(&result)?;
                Ok(ToolOutput::text(output))
            }
            LspOperation::DocumentSymbol => {
                let result = client.document_symbol(&path).await?;
                let output = serde_json::to_string_pretty(&result)?;
                Ok(ToolOutput::text(output))
            }
            LspOperation::WorkspaceSymbol => {
                Ok(ToolOutput::text("WorkspaceSymbol not fully supported yet (missing query parameter)"))
            }
            LspOperation::GetDiagnostics => {
                let result = client.get_diagnostics(&path).await?;
                let output = serde_json::to_string_pretty(&result)?;
                Ok(ToolOutput::text(output))
            }
            _ => {
                Ok(ToolOutput::text(format!("LSP operation {:?} not implemented yet", tool.operation)))
            }
        }
    }
}
