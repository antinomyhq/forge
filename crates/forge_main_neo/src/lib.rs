mod domain;
mod event_reader;
mod executor;
mod history;
mod run;
mod widgets;

use lazy_static::lazy_static;

lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}

pub use run::run;
