//! Workflow definitions for CI/CD

mod ci;
mod labels;
mod release_drafter;
mod release_publish;

pub use ci::*;
pub use labels::*;
pub use release_drafter::*;
pub use release_publish::*;
