use anyhow::Result;
use tonic::transport::Channel;
use url::Url;

/// Wrapper for a shared gRPC channel to the workspace server
///
/// This struct manages a lazily-connected gRPC channel that can be cheaply
/// cloned and shared across multiple gRPC clients.
#[derive(Clone)]
pub struct ForgeGrpcClient {
    channel: Channel,
}

impl ForgeGrpcClient {
    /// Creates a new gRPC client with a lazy connection to the specified server
    ///
    /// # Arguments
    /// * `server_url` - The URL of the gRPC server
    ///
    /// # Errors
    /// Returns an error if the channel cannot be created
    pub fn new(server_url: &Url) -> Result<Self> {
        let mut channel = Channel::from_shared(server_url.to_string())?.concurrency_limit(256);

        // Enable TLS for https URLs (webpki-roots is faster than native-roots)
        if server_url.scheme().contains("https") {
            let tls_config = tonic::transport::ClientTlsConfig::new().with_webpki_roots();
            channel = channel.tls_config(tls_config)?;
        }

        let channel = channel.connect_lazy();

        Ok(Self { channel })
    }

    /// Returns a clone of the underlying gRPC channel
    ///
    /// Channels are cheap to clone and can be shared across multiple clients.
    pub fn channel(&self) -> Channel {
        self.channel.clone()
    }
}
