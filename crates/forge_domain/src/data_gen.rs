use std::path::PathBuf;

use derive_setters::Setters;
use serde::{Deserialize, Serialize};

/// Parameters for data generation operations
///
/// This struct encapsulates the configuration parameters needed for generating
/// data in various contexts. It provides control over the amount of data to
/// generate, formatting options, and other generation-specific settings.
#[derive(Debug, Clone, Serialize, Deserialize, Setters, PartialEq, fake::Dummy)]
#[setters(into, strip_option)]
pub struct DataGenerationParameters {
    /// Path to input JSONL file for data generation
    pub input: PathBuf,

    /// Path to JSON schema file for LLM tool definition
    pub schema: PathBuf,

    /// Path to Handlebars template file for system prompt
    pub system_prompt: Option<PathBuf>,

    /// Path to Handlebars template file for user prompt
    pub user_prompt: Option<PathBuf>,

    /// Maximum number of concurrent LLM requests
    pub concurrency: usize,
}
