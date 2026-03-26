use std::path::PathBuf;
use std::sync::Arc;

use forge_config::{ConfigReader, ForgeConfig, ModelConfig};
use forge_domain::{
    AppConfigOperation, AppConfigRepository, AutoDumpFormat, Environment, HttpConfig, RetryConfig,
    SessionConfig, TlsBackend, TlsVersion,
};
use tracing::{debug, error};
use url::Url;

/// Converts a [`ModelConfig`] into a domain-level [`SessionConfig`].
fn to_session_config(mc: &ModelConfig) -> SessionConfig {
    SessionConfig {
        provider_id: mc.provider_id.clone(),
        model_id: mc.model_id.clone(),
    }
}

/// Converts a [`forge_config::TlsVersion`] into a [`forge_domain::TlsVersion`].
fn to_tls_version(v: forge_config::TlsVersion) -> TlsVersion {
    match v {
        forge_config::TlsVersion::V1_0 => TlsVersion::V1_0,
        forge_config::TlsVersion::V1_1 => TlsVersion::V1_1,
        forge_config::TlsVersion::V1_2 => TlsVersion::V1_2,
        forge_config::TlsVersion::V1_3 => TlsVersion::V1_3,
    }
}

/// Converts a [`forge_config::TlsBackend`] into a [`forge_domain::TlsBackend`].
fn to_tls_backend(b: forge_config::TlsBackend) -> TlsBackend {
    match b {
        forge_config::TlsBackend::Default => TlsBackend::Default,
        forge_config::TlsBackend::Rustls => TlsBackend::Rustls,
    }
}

/// Converts a [`forge_config::HttpConfig`] into a [`forge_domain::HttpConfig`].
fn to_http_config(h: forge_config::HttpConfig) -> HttpConfig {
    HttpConfig {
        connect_timeout: h.connect_timeout_secs,
        read_timeout: h.read_timeout_secs,
        pool_idle_timeout: h.pool_idle_timeout_secs,
        pool_max_idle_per_host: h.pool_max_idle_per_host,
        max_redirects: h.max_redirects,
        hickory: h.hickory,
        tls_backend: to_tls_backend(h.tls_backend),
        min_tls_version: h.min_tls_version.map(to_tls_version),
        max_tls_version: h.max_tls_version.map(to_tls_version),
        adaptive_window: h.adaptive_window,
        keep_alive_interval: h.keep_alive_interval_secs,
        keep_alive_timeout: h.keep_alive_timeout_secs,
        keep_alive_while_idle: h.keep_alive_while_idle,
        accept_invalid_certs: h.accept_invalid_certs,
        root_cert_paths: h.root_cert_paths,
    }
}

/// Converts a [`forge_config::RetryConfig`] into a
/// [`forge_domain::RetryConfig`].
fn to_retry_config(r: forge_config::RetryConfig) -> RetryConfig {
    RetryConfig {
        initial_backoff_ms: r.initial_backoff_ms,
        min_delay_ms: r.min_delay_ms,
        backoff_factor: r.backoff_factor,
        max_retry_attempts: r.max_attempts,
        retry_status_codes: r.status_codes,
        max_delay: r.max_delay_secs,
        suppress_retry_errors: r.suppress_errors,
    }
}

/// Converts a [`forge_config::AutoDumpFormat`] into a
/// [`forge_domain::AutoDumpFormat`].
fn to_auto_dump_format(f: forge_config::AutoDumpFormat) -> AutoDumpFormat {
    match f {
        forge_config::AutoDumpFormat::Json => AutoDumpFormat::Json,
        forge_config::AutoDumpFormat::Html => AutoDumpFormat::Html,
    }
}

/// Builds a [`forge_domain::Environment`] entirely from a [`ForgeConfig`],
/// mapping every config field to its corresponding environment field.
///
/// Infrastructure-only fields (`os`, `pid`, `cwd`, `home`, `shell`,
/// `base_path`, `forge_api_url`) are resolved using the same approach as the
/// infra layer: OS APIs and well-known fallbacks.
fn to_environment(fc: ForgeConfig) -> Environment {
    Environment {
        // --- Infrastructure-derived fields ---
        os: std::env::consts::OS.to_string(),
        pid: std::process::id(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        home: dirs::home_dir(),
        shell: if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        },
        base_path: dirs::home_dir()
            .map(|h| h.join("forge"))
            .unwrap_or_else(|| PathBuf::from(".").join("forge")),

        // --- ForgeConfig-mapped fields ---
        retry_config: fc.retry.map(to_retry_config).unwrap_or_default(),
        max_search_lines: fc.max_search_lines,
        max_search_result_bytes: fc.max_search_result_bytes,
        fetch_truncation_limit: fc.max_fetch_chars,
        stdout_max_prefix_length: fc.max_stdout_prefix_lines,
        stdout_max_suffix_length: fc.max_stdout_suffix_lines,
        stdout_max_line_length: fc.max_stdout_line_chars,
        max_line_length: fc.max_line_chars,
        max_read_size: fc.max_read_lines,
        max_file_read_batch_size: fc.max_file_read_batch_size,
        http: fc.http.map(to_http_config).unwrap_or_default(),
        max_file_size: fc.max_file_size_bytes,
        max_image_size: fc.max_image_size_bytes,
        tool_timeout: fc.tool_timeout_secs,
        auto_open_dump: fc.auto_open_dump,
        debug_requests: fc.debug_requests,
        custom_history_path: fc.custom_history_path,
        max_conversations: fc.max_conversations,
        sem_search_limit: fc.max_sem_search_results,
        sem_search_top_k: fc.sem_search_top_k,
        service_url: Url::parse(fc.services_url.as_str())
            .unwrap_or_else(|_| Url::parse("http://api.forgecode.dev").unwrap()),
        max_extensions: fc.max_extensions,
        auto_dump: fc.auto_dump.map(to_auto_dump_format),
        parallel_file_reads: fc.max_parallel_file_reads,
        model_cache_ttl: fc.model_cache_ttl_secs,
        session: fc.session.as_ref().map(to_session_config),
        commit: fc.commit.as_ref().map(to_session_config),
        suggest: fc.suggest.as_ref().map(to_session_config),
    }
}

/// Applies a single [`AppConfigOperation`] directly onto a [`ForgeConfig`]
/// in-place.
fn apply_op(op: AppConfigOperation, fc: &mut ForgeConfig) {
    match op {
        AppConfigOperation::SetProvider(provider_id) => {
            let pid = provider_id.as_ref().to_string();
            fc.session = Some(match fc.session.take() {
                Some(mc) => mc.provider_id(pid),
                None => ModelConfig::default().provider_id(pid),
            });
        }
        AppConfigOperation::SetModel(provider_id, model_id) => {
            let pid = provider_id.as_ref().to_string();
            let mid = model_id.to_string();
            fc.session = Some(match fc.session.take() {
                Some(mc) if mc.provider_id.as_deref() == Some(&pid) => mc.model_id(mid),
                _ => ModelConfig::default().provider_id(pid).model_id(mid),
            });
        }
        AppConfigOperation::SetCommitConfig(commit) => {
            fc.commit = commit
                .provider
                .as_ref()
                .zip(commit.model.as_ref())
                .map(|(pid, mid)| {
                    ModelConfig::default()
                        .provider_id(pid.as_ref().to_string())
                        .model_id(mid.to_string())
                });
        }
        AppConfigOperation::SetSuggestConfig(suggest) => {
            fc.suggest = Some(
                ModelConfig::default()
                    .provider_id(suggest.provider.as_ref().to_string())
                    .model_id(suggest.model.to_string()),
            );
        }
    }
}

/// Repository for managing application configuration with caching support.
///
/// Uses [`ForgeConfig::read`] and [`ForgeConfig::write`] for all file I/O and
/// maintains an in-memory cache to reduce disk access.
pub struct ForgeConfigRepository {
    cache: Arc<std::sync::Mutex<Option<ForgeConfig>>>,
}

impl ForgeConfigRepository {
    pub fn new() -> Self {
        Self { cache: Arc::new(std::sync::Mutex::new(None)) }
    }

    /// Reads [`ForgeConfig`] from disk via [`ForgeConfig::read`].
    fn read_from_disk() -> ForgeConfig {
        match ForgeConfig::read() {
            Ok(config) => {
                debug!(config = ?config, "read .forge.toml");
                config
            }
            Err(e) => {
                // NOTE: This should never-happen
                error!(error = ?e, "Failed to read config file. Using default config.");
                Default::default()
            }
        }
    }
}

#[async_trait::async_trait]
impl AppConfigRepository for ForgeConfigRepository {
    fn get_app_config(&self) -> Environment {
        let fc = {
            let mut cache = self.cache.lock().expect("cache mutex poisoned");
            if let Some(ref config) = *cache {
                config.clone()
            } else {
                let config = Self::read_from_disk();
                *cache = Some(config.clone());
                config
            }
        };

        to_environment(fc)
    }

    async fn update_app_config(&self, ops: Vec<AppConfigOperation>) -> anyhow::Result<()> {
        // Load the global config
        let mut fc = ConfigReader::default()
            .read_defaults()
            .read_global()
            .build()?;

        debug!(config = ?fc, "loaded config for update");

        // Apply each operation directly onto ForgeConfig
        debug!(?ops, "applying app config operations");
        for op in ops {
            apply_op(op, &mut fc);
        }

        // Persist
        fc.write()?;
        debug!(config = ?fc, "written .forge.toml");

        // Reset cache
        *self.cache.lock().expect("cache mutex poisoned") = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_config::{ForgeConfig, ModelConfig};
    use forge_domain::{AppConfigOperation, CommitConfig, ModelId, ProviderId, SuggestConfig};
    use pretty_assertions::assert_eq;

    use super::apply_op;

    #[test]
    fn test_apply_op_set_provider_creates_session_when_absent() {
        let mut fixture = ForgeConfig::default();
        apply_op(
            AppConfigOperation::SetProvider(ProviderId::from("anthropic".to_string())),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(ModelConfig::default().provider_id("anthropic".to_string())),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_provider_updates_existing_session_keeping_model() {
        let mut fixture = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        apply_op(
            AppConfigOperation::SetProvider(ProviderId::from("anthropic".to_string())),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("anthropic".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_model_for_matching_provider_updates_model() {
        let mut fixture = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-3.5".to_string()),
            ),
            ..Default::default()
        };
        apply_op(
            AppConfigOperation::SetModel(
                ProviderId::from("openai".to_string()),
                ModelId::new("gpt-4"),
            ),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_model_for_different_provider_replaces_session() {
        let mut fixture = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4".to_string()),
            ),
            ..Default::default()
        };
        apply_op(
            AppConfigOperation::SetModel(
                ProviderId::from("anthropic".to_string()),
                ModelId::new("claude-3"),
            ),
            &mut fixture,
        );
        let expected = ForgeConfig {
            session: Some(
                ModelConfig::default()
                    .provider_id("anthropic".to_string())
                    .model_id("claude-3".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_commit_config() {
        let mut fixture = ForgeConfig::default();
        let commit = CommitConfig::default()
            .provider(ProviderId::from("openai".to_string()))
            .model(ModelId::new("gpt-4o"));
        apply_op(AppConfigOperation::SetCommitConfig(commit), &mut fixture);
        let expected = ForgeConfig {
            commit: Some(
                ModelConfig::default()
                    .provider_id("openai".to_string())
                    .model_id("gpt-4o".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }

    #[test]
    fn test_apply_op_set_suggest_config() {
        let mut fixture = ForgeConfig::default();
        let suggest = SuggestConfig {
            provider: ProviderId::from("anthropic".to_string()),
            model: ModelId::new("claude-3-haiku"),
        };
        apply_op(AppConfigOperation::SetSuggestConfig(suggest), &mut fixture);
        let expected = ForgeConfig {
            suggest: Some(
                ModelConfig::default()
                    .provider_id("anthropic".to_string())
                    .model_id("claude-3-haiku".to_string()),
            ),
            ..Default::default()
        };
        assert_eq!(fixture, expected);
    }
}
