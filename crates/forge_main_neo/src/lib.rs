mod domain;
mod entrypoint;
mod event_reader;
mod executor;
mod run;
mod widgets;

pub static TRACKER: OnceCell<forge_tracker::Tracker> = OnceCell::new();

pub use entrypoint::main_neo;
use once_cell::sync::OnceCell;
pub use run::run;
