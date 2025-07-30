use anyhow::{Context, Result};
use forge_domain::{HttpConfig, TlsVersion};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Enhanced retry configuration for TLS fallback
#[derive(Debug, Clone)]
pub struct TlsRetryConfig {
    /// Maximum number of connection attempts per TLS version
    pub max_attempts_per_version: usize,
    /// Delay between retry attempts
    pub retry_delay: Duration,
    /// Whether to use exponential backoff
    pub exponential_backoff: bool,
    /// Maximum backoff delay
    pub max_backoff_delay: Duration,
}

impl Default for TlsRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts_per_version: 3,
            retry_delay: Duration::from_millis(500),
            exponential_backoff: true,
            max_backoff_delay: Duration::from_secs(5),
        }
    }
}

/// Result of a TLS connection attempt
#[derive(Debug)]
pub struct TlsConnectionResult {
    pub success: bool,
    pub version_used: Option<TlsVersion>,
    pub attempts_made: usize,
    pub error: Option<anyhow::Error>,
}

impl TlsConnectionResult {
    pub fn success(version: TlsVersion, attempts: usize) -> Self {
        Self {
            success: true,
            version_used: Some(version),
            attempts_made: attempts,
            error: None,
        }
    }

    pub fn failure(error: anyhow::Error, attempts: usize) -> Self {
        Self {
            success: false,
            version_used: None,
            attempts_made: attempts,
            error: Some(error),
        }
    }
}

/// Retry handler for TLS connections
pub struct TlsRetryHandler {
    config: TlsRetryConfig,
}

impl TlsRetryHandler {
    pub fn new(config: TlsRetryConfig) -> Self {
        Self { config }
    }

    /// Execute a connection attempt with retry logic
    pub async fn execute_with_retry<F, Fut, T>(
        &self,
        version: &TlsVersion,
        mut operation: F,
    ) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_error = None;
        let mut delay = self.config.retry_delay;

        for attempt in 1..=self.config.max_attempts_per_version {
            debug!(
                "TLS connection attempt {} of {} with version {}",
                attempt, self.config.max_attempts_per_version, version
            );

            match operation().await {
                Ok(result) => {
                    info!("Successfully connected with TLS version {} after {} attempts", version, attempt);
                    return Ok(result);
                }
                Err(e) => {
                    warn!("TLS connection attempt {} failed with version {}: {}", attempt, version, e);
                    last_error = Some(e);

                    if attempt < self.config.max_attempts_per_version {
                        debug!("Waiting {:?} before retry", delay);
                        tokio::time::sleep(delay).await;

                        if self.config.exponential_backoff {
                            delay = std::cmp::min(delay * 2, self.config.max_backoff_delay);
                        }
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!(
                "Failed to connect with TLS version {} after {} attempts",
                version,
                self.config.max_attempts_per_version
            )
        }))
    }
}

/// Helper functions for TLS diagnostics
pub mod diagnostics {
    use super::*;

    /// Log TLS configuration for debugging
    pub fn log_tls_config(config: &HttpConfig) {
        info!("TLS Configuration:");
        info!("  Backend: {}", config.tls_backend);
        info!("  Min Version: {}", config.min_tls_version);
        info!("  Max Version: {}", config.max_tls_version);
        info!("  Fallback Enabled: {}", config.tls_fallback_enabled);
    }

    /// Check if a TLS error is likely recoverable with a different version
    pub fn is_tls_version_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        
        // Common TLS version mismatch error patterns
        let version_error_patterns = [
            "tls",
            "ssl",
            "handshake",
            "protocol",
            "version",
            "cipher",
            "alert",
            "unsupported",
            "incompatible",
        ];

        version_error_patterns.iter().any(|pattern| error_str.contains(pattern))
    }

    /// Suggest alternative TLS configuration based on error
    pub fn suggest_tls_fix(error: &anyhow::Error, current_config: &HttpConfig) -> Option<String> {
        if is_tls_version_error(error) {
            let mut suggestions = Vec::new();

            // Suggest trying different backend
            match current_config.tls_backend {
                forge_domain::TlsBackend::Rustls => {
                    suggestions.push("Try using native TLS backend: FORGE_HTTP_TLS_BACKEND=native");
                }
                forge_domain::TlsBackend::Native | forge_domain::TlsBackend::Default => {
                    suggestions.push("Try using rustls backend: FORGE_HTTP_TLS_BACKEND=rustls");
                }
            }

            // Suggest adjusting version constraints
            if current_config.min_tls_version != TlsVersion::Tls10 {
                suggestions.push("Try allowing older TLS versions (security risk): Set min_tls_version to Tls10");
            }

            if !current_config.tls_fallback_enabled {
                suggestions.push("Enable TLS fallback: Set tls_fallback_enabled to true");
            }

            if !suggestions.is_empty() {
                return Some(suggestions.join("\n"));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_retry_handler_success() {
        let handler = TlsRetryHandler::new(TlsRetryConfig::default());
        let mut attempt_count = 0;

        let result = handler
            .execute_with_retry(&TlsVersion::Tls13, || {
                attempt_count += 1;
                async move {
                    if attempt_count == 2 {
                        Ok("success")
                    } else {
                        Err(anyhow::anyhow!("Connection failed"))
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempt_count, 2);
    }

    #[test]
    fn test_tls_version_error_detection() {
        let tls_errors = vec![
            anyhow::anyhow!("TLS handshake failed"),
            anyhow::anyhow!("SSL protocol error"),
            anyhow::anyhow!("Unsupported TLS version"),
            anyhow::anyhow!("Cipher suite incompatible"),
        ];

        for error in tls_errors {
            assert!(diagnostics::is_tls_version_error(&error));
        }

        let non_tls_error = anyhow::anyhow!("Connection timeout");
        assert!(!diagnostics::is_tls_version_error(&non_tls_error));
    }
}