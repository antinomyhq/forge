mod can_track;
mod collect;
mod dispatch;
mod error;
mod event;
mod log;
mod error_tracking;

pub use can_track::VERSION;
pub use dispatch::Tracker;
use error::Result;
pub use event::{Event, EventKind, ErrorDetails};
pub use log::{init_tracing, Guard};
pub use error_tracking::{track_error, track_error_without_trace};
