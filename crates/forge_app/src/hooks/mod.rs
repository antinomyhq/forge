mod compaction;
mod doom_loop;
mod title_generation;
mod tracing;
pub mod verification_reminder;

use forge_domain::Hook;

pub use compaction::CompactionHandler;
pub use doom_loop::DoomLoopDetector;
pub use title_generation::TitleGenerationHandler;
pub use tracing::TracingHandler;

pub fn default() -> Hook {
    Hook::default().on_request(DoomLoopDetector::default())
}
