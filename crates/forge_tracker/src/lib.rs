mod can_track;
mod collect;
mod dispatch;
mod error;
mod event;
mod log;
mod performance;
pub use can_track::VERSION;
pub use dispatch::Tracker;
use error::Result;
pub use event::{Event, EventKind, ToolCallPayload, ToolType};
pub use log::{Guard, init_tracing};
pub use performance::{
    MemoryTracker, PerformanceTimer, TimingManager, get_memory_usage, start_timing,
    track_clone_operation,
};
