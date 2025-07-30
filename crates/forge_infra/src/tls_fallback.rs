use std::time::Duration;

use anyhow::{Context, Result};
use forge_domain::{HttpConfig, TlsBackend, TlsVersion};
use reqwest::{Client, ClientBuilder};
use tracing::{debug, warn};

/// Builds an HTTP client with TLS version fallback support
pub struct TlsClientBuilder {
    config: HttpConfig,
}

impl TlsClientBuilder {
    pub fn new(config: HttpConfig) -> Self {
        Self { config }
    }

    /// Build a client with TLS fallback support
    pub async fn build_with_fallback(&self) -> Result<Client> {
        if !self.config.tls_fallback_enabled {
            // If fallback is disabled, just build with the configured settings
            return self
                .build_client(None)
                .context("Failed to build HTTP client");
        }

        // Get the list of TLS versions to try based on configuration
        let versions_to_try = self.get_tls_versions_to_try();

        let mut last_error = None;
        for version in &versions_to_try {
            debug!("Attempting to build client with TLS version: {}", version);
            match self.build_client(Some(version.clone())) {
                Ok(client) => {
                    debug!("Successfully built client with TLS version: {}", version);
                    return Ok(client);
                }
                Err(e) => {
                    warn!("Failed to build client with TLS version {}: {}", version, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| anyhow::anyhow!("Failed to build client with any TLS version")))
    }

    /// Build a client with specific TLS version settings
    fn build_client(&self, tls_version: Option<TlsVersion>) -> Result<Client> {
        let mut client = ClientBuilder::new()
            .connect_timeout(Duration::from_secs(self.config.connect_timeout))
            .read_timeout(Duration::from_secs(self.config.read_timeout))
            .pool_idle_timeout(Duration::from_secs(self.config.pool_idle_timeout))
            .pool_max_idle_per_host(self.config.pool_max_idle_per_host)
            .redirect(reqwest::redirect::Policy::limited(
                self.config.max_redirects,
            ))
            .hickory_dns(self.config.hickory)
            .connection_verbose(true);

        // Apply TLS backend configuration
        client = match self.config.tls_backend {
            TlsBackend::Rustls => {
                // Configure rustls with version support
                client = client.use_rustls_tls();

                if let Some(version) = tls_version {
                    client = self.configure_rustls_version(client, version)?;
                }
                client
            }
            TlsBackend::Native => {
                // Configure native TLS with version support
                client = client.use_native_tls();

                if let Some(version) = tls_version {
                    client = self.configure_native_tls_version(client, version)?;
                }
                client
            }
            TlsBackend::Default => {
                // Use native TLS by default for better compatibility
                client = client.use_native_tls();

                if let Some(version) = tls_version {
                    client = self.configure_native_tls_version(client, version)?;
                }
                client
            }
        };

        client.build().context("Failed to build HTTP client")
    }

    /// Configure rustls with specific TLS version
    fn configure_rustls_version(
        &self,
        client: ClientBuilder,
        version: TlsVersion,
    ) -> Result<ClientBuilder> {
        // Note: Direct TLS version configuration for rustls requires access to the
        // underlying rustls configuration. Since reqwest doesn't expose this
        // directly, we'll need to use a custom approach or accept the default
        // rustls configuration which supports TLS 1.2 and 1.3
        match version {
            TlsVersion::Tls10 | TlsVersion::Tls11 => {
                // rustls doesn't support TLS 1.0 or 1.1
                Err(anyhow::anyhow!(
                    "rustls does not support TLS versions below 1.2"
                ))
            }
            TlsVersion::Tls12 | TlsVersion::Tls13 | TlsVersion::Auto => {
                // rustls supports TLS 1.2 and 1.3 by default
                Ok(client)
            }
        }
    }

    /// Configure native TLS with specific version
    fn configure_native_tls_version(
        &self,
        client: ClientBuilder,
        version: TlsVersion,
    ) -> Result<ClientBuilder> {
        // Note: Native TLS version configuration depends on the platform
        // This is a placeholder - actual implementation would need platform-specific
        // code
        match version {
            TlsVersion::Auto => {
                // Let the native TLS library negotiate the best version
                Ok(client)
            }
            _ => {
                // Platform-specific TLS version configuration would go here
                // For now, we'll accept any version and let the native library handle it
                warn!("Specific TLS version configuration for native TLS is platform-dependent");
                Ok(client)
            }
        }
    }

    /// Get the list of TLS versions to try based on configuration
    fn get_tls_versions_to_try(&self) -> Vec<TlsVersion> {
        let mut versions = Vec::new();

        // If max version is Auto, try from newest to oldest within the allowed range
        if self.config.max_tls_version == TlsVersion::Auto {
            // Try versions from newest to oldest
            versions.push(TlsVersion::Tls13);
            versions.push(TlsVersion::Tls12);

            // Only include older versions if min_tls_version allows
            match self.config.min_tls_version {
                TlsVersion::Tls11 | TlsVersion::Tls10 => {
                    versions.push(TlsVersion::Tls11);
                }
                _ => {}
            }

            if self.config.min_tls_version == TlsVersion::Tls10 {
                versions.push(TlsVersion::Tls10);
            }
        } else {
            // Start with the max version and work down to min
            versions.push(self.config.max_tls_version.clone());

            // Add intermediate versions if needed
            if self.should_try_version(&TlsVersion::Tls13) {
                versions.push(TlsVersion::Tls13);
            }
            if self.should_try_version(&TlsVersion::Tls12) {
                versions.push(TlsVersion::Tls12);
            }
            if self.should_try_version(&TlsVersion::Tls11) {
                versions.push(TlsVersion::Tls11);
            }
            if self.should_try_version(&TlsVersion::Tls10) {
                versions.push(TlsVersion::Tls10);
            }
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        versions.retain(|v| seen.insert(v.clone()));

        versions
    }

    /// Check if a version should be tried based on min/max configuration
    fn should_try_version(&self, version: &TlsVersion) -> bool {
        let version_num = self.version_to_number(version);
        let min_num = self.version_to_number(&self.config.min_tls_version);
        let max_num = self.version_to_number(&self.config.max_tls_version);

        version_num >= min_num && version_num <= max_num
    }

    /// Convert TLS version to a comparable number
    fn version_to_number(&self, version: &TlsVersion) -> u8 {
        match version {
            TlsVersion::Tls10 => 1,
            TlsVersion::Tls11 => 2,
            TlsVersion::Tls12 => 3,
            TlsVersion::Tls13 => 4,
            TlsVersion::Auto => 5, // Highest priority
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_tls_version_ordering() {
        let config = HttpConfig {
            min_tls_version: TlsVersion::Tls11,
            max_tls_version: TlsVersion::Auto,
            tls_fallback_enabled: true,
            ..Default::default()
        };

        let builder = TlsClientBuilder::new(config);
        let versions = builder.get_tls_versions_to_try();

        // Should try from newest to oldest, respecting min version
        assert_eq!(
            versions,
            vec![TlsVersion::Tls13, TlsVersion::Tls12, TlsVersion::Tls11,]
        );
    }

    #[test]
    fn test_specific_version_range() {
        let config = HttpConfig {
            min_tls_version: TlsVersion::Tls12,
            max_tls_version: TlsVersion::Tls13,
            tls_fallback_enabled: true,
            ..Default::default()
        };

        let builder = TlsClientBuilder::new(config);
        let versions = builder.get_tls_versions_to_try();

        assert_eq!(versions, vec![TlsVersion::Tls13, TlsVersion::Tls12,]);
    }
}
