use std::sync::{Arc, OnceLock};

use tonic::transport::Channel;
use url::Url;

/// Wrapper for a shared gRPC channel to the workspace server
///
/// This struct manages a lazily-connected gRPC channel that can be cheaply
/// cloned and shared across multiple gRPC clients. The channel is only created
/// on first access.
#[derive(Clone)]
pub struct ForgeGrpcClient {
    server_url: Arc<Url>,
    channel: Arc<OnceLock<Channel>>,
}

impl ForgeGrpcClient {
    /// Creates a new gRPC client that will lazily connect on first use
    ///
    /// # Arguments
    /// * `server_url` - The URL of the gRPC server
    pub fn new(server_url: Url) -> Self {
        Self {
            server_url: Arc::new(server_url),
            channel: Arc::new(OnceLock::new()),
        }
    }

    /// Returns a clone of the underlying gRPC channel
    ///
    /// Channels are cheap to clone and can be shared across multiple clients.
    /// The channel is created on first call and cached for subsequent calls.
    pub fn channel(&self) -> Channel {
        self.channel
            .get_or_init(|| {
                let mut channel = Channel::from_shared(self.server_url.to_string())
                    .expect("Invalid server URL")
                    .concurrency_limit(256);

                // Enable TLS for https URLs (webpki-roots is faster than native-roots)
                if self.server_url.scheme().contains("https") {
                    let tls_config = tonic::transport::ClientTlsConfig::new().with_webpki_roots();
                    channel = channel
                        .tls_config(tls_config)
                        .expect("Failed to configure TLS");
                }

                channel.connect_lazy()
            })
            .clone()
    }
}
