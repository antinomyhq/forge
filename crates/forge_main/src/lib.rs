mod auto_update;
mod banner;
mod cli;
mod completer;
mod editor;
mod info;
mod input;
mod model;
mod prompt;
mod state;
mod tools_display;
mod ui;

pub use auto_update::update_forge;
pub use cli::Cli;
use lazy_static::lazy_static;
pub use ui::UI;
lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}
