use std::path::PathBuf;

use tracing::debug;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{self};

use crate::Tracker;

pub fn init_tracing(log_path: PathBuf, tracker: Tracker) -> anyhow::Result<Guard> {
    debug!(path = %log_path.display(), "Initializing logging system in JSON format");

    let (non_blocking, guard) = tracing_appender::non_blocking(PostHogWriter::new(tracker));

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("FORGE_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("forge=debug")),
        )
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_thread_ids(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_writer(non_blocking)
        .init();

    debug!("JSON logging system initialized successfully");
    Ok(Guard(guard))
}

pub struct Guard(#[allow(dead_code)] WorkerGuard);

struct PostHogWriter {
    tracker: Tracker,
    runtime: tokio::runtime::Runtime,
}

impl PostHogWriter {
    pub fn new(tracker: Tracker) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");
        Self { tracker, runtime }
    }
}

impl std::io::Write for PostHogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let tracker = self.tracker.clone();
        let event_kind = crate::EventKind::Trace(buf.to_vec());
        self.runtime.spawn(async move {
            let _ = tracker.dispatch(event_kind).await;
        });
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
