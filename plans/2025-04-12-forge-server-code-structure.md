# Forge Server Implementation - Code Structure

## Overview
This document provides a detailed code structure for the `forge_server` crate implementation, complementing the high-level plan in `2025-04-12-forge-server-jsonrpc-implementation.md`.

## Cargo.toml Structure

```toml
[package]
name = "forge_server"
version = "0.1.0"
edition = "2021"
description = "JSON-RPC server for Forge API"
license = "MIT"

[dependencies]
# Workspace dependencies
anyhow.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true

# Internal crates
forge_api.workspace = true
forge_domain.workspace = true
forge_stream.workspace = true

# JSON-RPC dependencies
jsonrpc-core = "18.0.0"
jsonrpc-stdio-server = "18.0.0"

# Other dependencies
futures.workspace = true
uuid.workspace = true
```

## Module Structure

### lib.rs

```rust
//! JSON-RPC server implementation for the Forge API.

mod error;
mod methods;
mod serialization;
mod server;
mod stream_handler;

pub use error::*;
pub use server::*;
```

### server.rs

This module will contain the main server implementation:

```rust
//! JSON-RPC server implementation

use std::sync::Arc;
use anyhow::Result;
use jsonrpc_core::{IoHandler, MetaIoHandler};
use jsonrpc_stdio_server::ServerBuilder;
use forge_api::ForgeAPI;
use forge_domain::*;

use crate::methods::*;
use crate::stream_handler::StreamManager;

/// Configuration for the Forge JSON-RPC server
pub struct ForgeServerConfig {
    /// Whether to use restricted mode for file operations
    pub restricted: bool,
}

/// The main Forge JSON-RPC server
pub struct ForgeServer {
    api: Arc<ForgeAPI<forge_services::ForgeServices<forge_infra::ForgeInfra>>>,
    stream_manager: Arc<StreamManager>,
}

impl ForgeServer {
    /// Create a new server with the given configuration
    pub fn new(config: ForgeServerConfig) -> Self {
        // Implementation details
    }

    /// Start the server and block until it completes
    pub fn start(&self) -> Result<()> {
        // Implementation details
    }
}
```

### methods.rs

This module will contain method handlers for the JSON-RPC server, directly calling the ForgeAPI:

```rust
//! JSON-RPC method handlers implementation

use anyhow::Result;
use forge_domain::*;
use jsonrpc_core::{IoHandler, Error as JsonRpcError, ErrorCode, Params, Value};
use std::path::Path;
use std::sync::Arc;

use crate::error::map_error;
use crate::serialization::*;
use crate::stream_handler::StreamManager;

/// Registers all method handlers with the JSON-RPC server
pub fn register_methods(io: &mut IoHandler, api: Arc<dyn API>, stream_manager: Arc<StreamManager>) {
    // Register all the API methods directly
    
    // suggestions
    let api_clone = api.clone();
    io.add_method("suggestions", move |_params| {
        let api = api_clone.clone();
        async move {
            let suggestions = api.suggestions().await.map_err(map_error)?;
            Ok(serde_json::to_value(suggestions).unwrap())
        }
    });
    
    // tools
    let api_clone = api.clone();
    io.add_method("tools", move |_params| {
        let api = api_clone.clone();
        async move {
            let tools = api.tools().await;
            Ok(serde_json::to_value(tools).unwrap())
        }
    });
    
    // Register other methods following the same pattern...
    // models, environment, init, etc.
    
    // chat - special handling for streaming response
    let api_clone = api.clone();
    let stream_manager_clone = stream_manager.clone();
    io.add_method("chat", move |params: Params| {
        let api = api_clone.clone();
        let stream_manager = stream_manager_clone.clone();
        async move {
            let chat_request: ChatRequest = params.parse().map_err(|e| {
                JsonRpcError {
                    code: ErrorCode::InvalidParams,
                    message: format!("Invalid parameters: {}", e),
                    data: None,
                }
            })?;
            
            let stream = api.chat(chat_request).await.map_err(map_error)?;
            let stream_id = stream_manager.register_stream(stream).await;
            
            Ok(serde_json::to_value(stream_id).unwrap())
        }
    });
    
    // Stream subscription methods
    let stream_manager_clone = stream_manager.clone();
    io.add_method("chatSubscribe", move |params: Params| {
        let stream_manager = stream_manager_clone.clone();
        async move {
            let stream_id: String = params.parse().map_err(|e| {
                JsonRpcError {
                    code: ErrorCode::InvalidParams,
                    message: format!("Invalid parameters: {}", e),
                    data: None,
                }
            })?;
            
            Ok(serde_json::to_value(true).unwrap()) // Subscription success
        }
    });
    
    // Additional methods would be registered similarly
}
```

### error.rs

This module will handle error mapping:

```rust
//! Error handling for JSON-RPC

use anyhow::Error as AnyhowError;
use jsonrpc_core::{Error as JsonRpcError, ErrorCode};

/// Map an anyhow error to a JSON-RPC error
pub fn map_error(error: AnyhowError) -> JsonRpcError {
    // Error mapping implementation
}
```

### serialization.rs

This module will handle serialization/deserialization for JSON-RPC compatibility:

```rust
//! Serialization helpers for JSON-RPC

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use forge_domain::*;

/// Register the necessary serialization/deserialization methods for domain types
pub fn register_serializers() {
    // Register any custom serializers if needed
}

/// Convert domain error to JSON-RPC compatible format
pub fn error_to_json(error: anyhow::Error) -> Value {
    json!({
        "message": error.to_string(),
        "details": format!("{:?}", error),
    })
}

// Helper functions for any special serialization cases
```

### stream_handler.rs

This module will handle streaming responses:

```rust
//! Stream handling for JSON-RPC

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use anyhow::Result;
use forge_domain::*;
use forge_stream::MpscStream;
use jsonrpc_core::futures::channel::mpsc;
use jsonrpc_core::futures::StreamExt;
use jsonrpc_core::types::Notification;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Manages active streams
pub struct StreamManager {
    streams: Mutex<HashMap<String, StreamHandle>>,
}

/// Handle for an active stream
struct StreamHandle {
    join_handle: JoinHandle<()>,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new() -> Self {
        // Implementation
    }

    /// Register a new stream and get its ID
    pub async fn register_stream(
        &self,
        stream: MpscStream<Result<AgentMessage<ChatResponse>, anyhow::Error>>,
    ) -> String {
        // Implementation
    }

    /// Remove a stream by ID
    pub fn remove_stream(&self, id: &str) -> bool {
        // Implementation
    }
}
```

## Binary Target

Additionally, we'll add a binary target for standalone usage:

```rust
// src/bin/forge-server.rs

use anyhow::Result;
use forge_server::{ForgeServer, ForgeServerConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    
    // Initialize the server
    let config = ForgeServerConfig {
        restricted: false,
    };
    
    let server = ForgeServer::new(config);
    
    // Start the server
    server.start()?;
    
    Ok(())
}
```

This code structure provides a comprehensive starting point for implementing the JSON-RPC server that wraps the ForgeAPI.