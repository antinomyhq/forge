use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Configuration for the Forge HTTP server
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// The address to bind to
    pub host: IpAddr,
    /// The port to bind to
    pub port: u16,
    /// Whether to enable CORS
    pub enable_cors: bool,
    /// Whether to use restricted mode for the forge API
    pub restricted: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 3000,
            enable_cors: true,
            restricted: true,
        }
    }
}

/// Helper to get socket address from config
pub(crate) fn get_socket_addr(config: &ServerConfig) -> SocketAddr {
    SocketAddr::new(config.host, config.port)
}
