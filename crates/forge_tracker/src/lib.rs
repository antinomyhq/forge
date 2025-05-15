mod can_track;
mod collect;
mod dispatch;
mod error;
mod error_reporting;
mod event;
mod log;

// External crates used by specific modules
pub use can_track::VERSION;
pub use dispatch::Tracker;
use error::Result;
pub use error_reporting::{install_panic_hook, GithubIssueCreator, PanicReport, SystemInfo};
pub use event::{Event, EventKind, ToolCallPayload};
pub use log::{init_tracing, Guard};
