//! JSON-RPC server implementation

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use forge_api::ForgeAPI;
use forge_infra::ForgeInfra;
use forge_services::ForgeServices;
use jsonrpc_core::IoHandler;
use jsonrpc_stdio_server::ServerBuilder;
use tracing::{error, info};

use crate::methods::register_methods;
use crate::stream_handler::StreamManager;

/// Configuration for the Forge JSON-RPC server
#[derive(Default)]
pub struct ForgeServerConfig {
    /// Whether to use restricted mode for file operations
    pub restricted: bool,

    /// Buffer size for notification channel
    /// Must be at least 1 to avoid panics with tokio::mpsc
    pub notification_buffer_size: usize,
}

impl ForgeServerConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self { restricted: false, notification_buffer_size: 100 }
    }

    /// Validates that the configuration is valid
    fn validate(&self) -> Self {
        // Ensure notification buffer size is at least 1
        let notification_buffer_size = std::cmp::max(1, self.notification_buffer_size);

        Self { restricted: self.restricted, notification_buffer_size }
    }

    /// Set the restricted mode
    pub fn with_restricted(mut self, restricted: bool) -> Self {
        self.restricted = restricted;
        self
    }

    /// Set the notification buffer size
    pub fn with_notification_buffer_size(mut self, size: usize) -> Self {
        self.notification_buffer_size = size;
        self
    }
}

/// The main Forge JSON-RPC server
pub struct ForgeServer {
    api: Arc<ForgeAPI<ForgeServices<ForgeInfra>>>,
    stream_manager: Arc<StreamManager>,
    config: ForgeServerConfig,
}

impl ForgeServer {
    /// Create a new server with the given configuration
    pub fn new(config: ForgeServerConfig) -> Result<Self> {
        info!("Initializing ForgeServer");

        // Validate the configuration
        let config = config.validate();

        // Initialize the ForgeAPI
        let api = ForgeAPI::init(config.restricted);

        // Create the stream manager for handling streaming responses
        let stream_manager = StreamManager::new();

        Ok(Self {
            api: Arc::new(api),
            stream_manager: Arc::new(stream_manager),
            config,
        })
    }

    /// Start the server and block until it completes
    pub fn start(&self) -> Result<()> {
        info!("Starting ForgeServer");

        // Create a new IoHandler
        let mut io = IoHandler::new();

        // Create a notification channel
        let (notification_tx, notification_rx) =
            tokio::sync::mpsc::channel(self.config.notification_buffer_size);

        // Set the notification sender in our stream manager
        self.stream_manager.set_notification_sender(notification_tx);

        // Register JSON-RPC method handlers
        info!("Registering JSON-RPC method handlers");
        register_methods(&mut io, self.api.clone(), self.stream_manager.clone());

        // Create shutdown mechanisms
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let shutdown_flag_clone = shutdown_flag.clone();

        // Clone the stream manager for the thread
        let stream_manager = self.stream_manager.clone();

        // Build the server
        let server_builder = ServerBuilder::new(io);
        let server = server_builder.build();

        // Execute the server
        info!("Starting JSON-RPC server");

        // Setup CTRL+C handler for graceful shutdown
        let flag = shutdown_flag.clone();
        let stream_manager_clone = self.stream_manager.clone();
        ctrlc::set_handler(move || {
            info!("Received shutdown signal (CTRL+C), starting graceful shutdown");
            // First shut down all streams gracefully
            stream_manager_clone.shutdown_all_streams();
            // Then set the flag to terminate the server
            flag.store(true, Ordering::SeqCst);
        })?;

        // Start the server in its own thread but make sure it has access to the tokio
        // runtime
        let runtime = tokio::runtime::Handle::current();

        let server_thread = std::thread::spawn(move || {
            // This runs the server on the current tokio runtime context
            runtime.block_on(async {
                // Signal when the server is ready
                info!("Server thread started");

                // Create a channel to signal when server is done
                let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

                // Run the server in a task
                let server_task = tokio::task::spawn(async move {
                    info!("JSON-RPC server ready to accept connections");

                    // Set up a notification forwarder to process messages from notification channel
                    // This is critical for stream updates to reach the client
                    let mut rx = notification_rx;
                    let notification_task = tokio::task::spawn(async move {
                        info!("Notification forwarder started");
                        while let Some(notification) = rx.recv().await {
                            info!(
                                "Forwarding notification [{}]: {}",
                                notification.method,
                                serde_json::to_string(&notification)
                                    .unwrap_or_else(|_| "[serialization error]".to_string())
                            );

                            // Convert to serialized JSON and write to stdout
                            match serde_json::to_string(&notification) {
                                Ok(json) => {
                                    // Log the raw output to help with debugging
                                    eprintln!("[DEBUG] Sending notification: {}", json);

                                    // Write to stdout and flush immediately
                                    println!("{}", json);
                                    std::io::stdout().flush().unwrap_or_else(|e| {
                                        error!("Failed to flush stdout: {}", e);
                                    });
                                }
                                Err(e) => {
                                    error!("Failed to serialize notification: {}", e);
                                }
                            }
                        }
                        info!("Notification forwarder stopped");
                    });

                    // Execute the server
                    futures::executor::block_on(server);

                    // Abort the notification task when the server is done
                    notification_task.abort();

                    // Signal we're done
                    let _ = shutdown_tx.send(());
                    info!("Server execution completed");
                });

                // Wait for shutdown signal or server completion
                loop {
                    tokio::select! {
                        // Check if shutdown was requested via CTRL+C
                        _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                            if shutdown_flag_clone.load(Ordering::SeqCst) {
                                info!("Shutdown requested - terminating server");
                                // Gracefully shut down all streams first
                                stream_manager.shutdown_all_streams();
                                // Abort the server task to initiate graceful shutdown
                                server_task.abort();
                                break;
                            }
                        }
                        // Check if server has already completed
                        recv_result = &mut shutdown_rx => {
                            match recv_result {
                                Ok(_) => {
                                    info!("Server execution completed naturally");
                                    break;
                                }
                                Err(_) => {
                                    // Sender was dropped but no message was sent (unusual)
                                    info!("Server shutdown unexpectedly");
                                    break;
                                }
                            }
                        }
                    }
                }

                info!("Server shutdown complete");
            });
        });

        // Wait for the server thread to complete
        let _ = server_thread.join();

        info!("ForgeServer shut down");
        Ok(())
    }
}
