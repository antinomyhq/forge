use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use lsp_types::{
    ClientCapabilities, InitializeParams, InitializeResult, InitializedParams,
    TextDocumentIdentifier, TextDocumentItem, Url, PublishDiagnosticsParams, Diagnostic
};
use serde_json::{Value, json};
use tokio::sync::{Mutex, oneshot};
use tokio::time::timeout;
use tracing::{info, error};

use crate::transport::{LspTransport, LspWriter};

pub struct LspClient {
    #[allow(dead_code)]
    transport: Arc<LspTransport>,
    writer: Arc<Mutex<LspWriter>>,
    next_id: AtomicI64,
    pending_requests: Arc<DashMap<i64, oneshot::Sender<Result<Value>>>>,
    root_uri: Url,
    diagnostics: Arc<DashMap<Url, Vec<Diagnostic>>>,
    open_files: Arc<DashMap<Url, bool>>,
}

impl LspClient {
    pub async fn new(command: &str, args: &[String], root_path: &std::path::Path) -> Result<Self> {
        let (transport, mut reader, writer) = LspTransport::new(command, args, Some(root_path))?;
        let transport = Arc::new(transport);
        let writer = Arc::new(Mutex::new(writer));
        let pending_requests = Arc::new(DashMap::new());
        let diagnostics = Arc::new(DashMap::new());
        let open_files = Arc::new(DashMap::new());

        let client = Self {
            transport: transport.clone(),
            writer: writer.clone(),
            next_id: AtomicI64::new(1),
            pending_requests: pending_requests.clone(),
            root_uri: Url::from_directory_path(root_path).map_err(|_| anyhow!("Invalid root path"))?,
            diagnostics: diagnostics.clone(),
            open_files: open_files.clone(),
        };

        // Start listening loop
        let pending_requests_clone = pending_requests.clone();
        let diagnostics_clone = diagnostics.clone();
        tokio::spawn(async move {
            loop {
                match reader.read_message().await {
                    Ok(Some(msg)) => {
                        if let Some(id) = msg.get("id").and_then(|id| id.as_i64()) {
                            // It's a response
                            if let Some((_, sender)) = pending_requests_clone.remove(&id) {
                                if let Some(error) = msg.get("error") {
                                     let _ = sender.send(Err(anyhow!("LSP Error: {}", error)));
                                } else if let Some(result) = msg.get("result") {
                                     let _ = sender.send(Ok(result.clone()));
                                } else {
                                     let _ = sender.send(Ok(Value::Null));
                                }
                            }
                        } else {
                            // Notification
                            if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
                                if method == "textDocument/publishDiagnostics" {
                                    if let Some(params) = msg.get("params") {
                                        match serde_json::from_value::<PublishDiagnosticsParams>(params.clone()) {
                                            Ok(params) => {
                                                diagnostics_clone.insert(params.uri, params.diagnostics);
                                            }
                                            Err(e) => {
                                                error!("Failed to parse publishDiagnostics params: {}", e);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Ok(None) => break, // EOF
                    Err(e) => {
                        error!("Error reading LSP message: {}", e);
                        break;
                    }
                }
            }

            // Connection closed, fail all pending requests
            let keys: Vec<_> = pending_requests_clone.iter().map(|r| *r.key()).collect();
            for key in keys {
                if let Some((_, sender)) = pending_requests_clone.remove(&key) {
                    let _ = sender.send(Err(anyhow!("LSP connection closed")));
                }
            }
        });

        client.initialize().await?;

        Ok(client)
    }

    async fn send_request<R: serde::de::DeserializeOwned>(&self, method: &str, params: Value) -> Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let (tx, rx) = oneshot::channel();
        self.pending_requests.insert(id, tx);

        {
            let mut w = self.writer.lock().await;
            w.write_message(&request).await?;
        }

        // Add 30s timeout for requests
        let response = match timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(res)) => res?,
            Ok(Err(_)) => {
                self.pending_requests.remove(&id);
                return Err(anyhow!("LSP request channel closed"));
            }
            Err(_) => {
                self.pending_requests.remove(&id);
                return Err(anyhow!("LSP request timed out"));
            }
        };

        serde_json::from_value(response).map_err(|e| anyhow!("Failed to parse response: {}", e))
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let mut w = self.writer.lock().await;
        w.write_message(&notification).await?;
        Ok(())
    }

    async fn initialize(&self) -> Result<()> {
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
                uri: self.root_uri.clone(),
                name: "root".to_string(),
            }]),
            #[allow(deprecated)]
            root_uri: Some(self.root_uri.clone()),
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };

        let result: InitializeResult = self.send_request("initialize", serde_json::to_value(params)?).await?;

        self.send_notification("initialized", serde_json::to_value(InitializedParams {})?).await?;

        info!("LSP Initialized: {:?}", result.capabilities);

        Ok(())
    }

    pub async fn did_open(&self, path: &std::path::Path, language_id: &str, content: String) -> Result<()> {
        let uri = Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path"))?;
        
        if self.open_files.contains_key(&uri) {
            return Ok(());
        }

        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: language_id.to_string(),
                version: 1,
                text: content,
            },
        };

        self.send_notification("textDocument/didOpen", serde_json::to_value(params)?).await?;
        self.open_files.insert(uri, true);
        Ok(())
    }

    pub async fn goto_definition(&self, path: &std::path::Path, line: u32, character: u32) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        let uri = Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path"))?;
        let params = lsp_types::GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: lsp_types::Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result: Option<lsp_types::GotoDefinitionResponse> = self.send_request("textDocument/definition", serde_json::to_value(params)?).await?;
        Ok(result)
    }

    pub async fn find_references(&self, path: &std::path::Path, line: u32, character: u32) -> Result<Option<Vec<lsp_types::Location>>> {
        let uri = Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path"))?;
        let params = lsp_types::ReferenceParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: lsp_types::Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: lsp_types::ReferenceContext { include_declaration: true },
        };

        let result: Option<Vec<lsp_types::Location>> = self.send_request("textDocument/references", serde_json::to_value(params)?).await?;
        Ok(result)
    }

    pub async fn hover(&self, path: &std::path::Path, line: u32, character: u32) -> Result<Option<lsp_types::Hover>> {
        let uri = Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path"))?;
        let params = lsp_types::HoverParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: lsp_types::Position { line, character },
            },
            work_done_progress_params: Default::default(),
        };

        let result: Option<lsp_types::Hover> = self.send_request("textDocument/hover", serde_json::to_value(params)?).await?;
        Ok(result)
    }

    pub async fn document_symbol(&self, path: &std::path::Path) -> Result<Option<lsp_types::DocumentSymbolResponse>> {
        let uri = Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path"))?;
        let params = lsp_types::DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result: Option<lsp_types::DocumentSymbolResponse> = self.send_request("textDocument/documentSymbol", serde_json::to_value(params)?).await?;
        Ok(result)
    }

    pub async fn workspace_symbol(&self, query: &str) -> Result<Option<Vec<lsp_types::SymbolInformation>>> {
        let params = lsp_types::WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result: Option<Vec<lsp_types::SymbolInformation>> = self.send_request("workspace/symbol", serde_json::to_value(params)?).await?;
        Ok(result)
    }

    pub async fn get_diagnostics(&self, path: &std::path::Path) -> Result<Vec<Diagnostic>> {
        let uri = Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path"))?;
        if let Some(diagnostics) = self.diagnostics.get(&uri) {
            Ok(diagnostics.clone())
        } else {
            Ok(vec![])
        }
    }
}
