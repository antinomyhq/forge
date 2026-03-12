mod config;
mod read;

pub use config::{AutoDumpFormat, CompactConfig, ForgeConfig, SummaryTag, UpdateConfig, UpdateFrequency};
pub use read::{read, read_as, ConfigReadError};
