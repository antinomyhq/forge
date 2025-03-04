mod can_track;
mod collect;
mod dispatch;
mod error;
mod error_tracking;
mod event;
mod log;

pub use can_track::VERSION;
pub use dispatch::Tracker;
use error::Result;
pub use error_tracking::{track_error, track_error_without_trace};
pub use event::{ErrorDetails, Event, EventKind};
pub use log::{init_tracing, Guard};
