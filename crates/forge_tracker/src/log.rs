use std::path::PathBuf;

use tracing::debug;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{self};

pub fn init_tracing(log_path: PathBuf) -> anyhow::Result<Guard> {
    debug!(path = %log_path.display(), "Initializing logging system");

    let append = tracing_appender::rolling::daily(log_path, "forge.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(append);

    // Try to initialize the global subscriber, but don't fail if it's already set
    // This allows tests to run in parallel without failing
    let subscriber = tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("FORGE_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("forge=debug")),
        )
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_thread_ids(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_ansi(true)
        .with_span_events(FmtSpan::ACTIVE)
        .with_writer(non_blocking)
        .finish();

    // Try to set the global default, but don't panic if it fails because it's
    // already set
    let _ = tracing::subscriber::set_global_default(subscriber);

    debug!("Logging system initialized successfully");
    Ok(Guard(guard))
}

pub struct Guard(#[allow(dead_code)] WorkerGuard);
