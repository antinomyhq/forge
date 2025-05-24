use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Enum representing all possible tool input types.
///
/// This enum contains variants for each type of input that can be passed to
/// tools in the application. Each variant corresponds to the input type for a
/// specific tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "tool", content = "args")]
pub enum ToolInput {
    /// Input for the file read tool
    #[serde(rename = "forge_tool_fs_read")]
    FSRead(FSReadInput),

    /// Input for the file write tool
    #[serde(rename = "forge_tool_fs_create")]
    FSWrite(FSWriteInput),

    /// Input for the file search tool
    #[serde(rename = "forge_tool_fs_search")]
    FSSearch(FSSearchInput),

    /// Input for the file remove tool
    #[serde(rename = "forge_tool_fs_remove")]
    FSRemove(FSRemoveInput),

    /// Input for the file patch tool
    #[serde(rename = "forge_tool_fs_patch")]
    FSPatch(FSPatchInput),

    /// Input for the file undo tool
    #[serde(rename = "forge_tool_fs_undo")]
    FSUndo(FSUndoInput),

    /// Input for the shell command tool
    #[serde(rename = "forge_tool_process_shell")]
    Shell(ShellInput),

    /// Input for the net fetch tool
    #[serde(rename = "forge_tool_net_fetch")]
    NetFetch(NetFetchInput),

    /// Input for the completion tool
    #[serde(rename = "forge_tool_attempt_completion")]
    AttemptCompletion(AttemptCompletionInput),
}

/// Input type for the file read tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FSReadInput {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,

    /// Optional start position in characters (0-based). If provided, reading
    /// will start from this character position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_char: Option<u64>,

    /// Optional end position in characters (inclusive). If provided, reading
    /// will end at this character position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_char: Option<u64>,
}

/// Input type for the file write tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FSWriteInput {
    /// The path of the file to write to (absolute path required)
    pub path: String,

    /// The content to write to the file. ALWAYS provide the COMPLETE intended
    /// content of the file, without any truncation or omissions. You MUST
    /// include ALL parts of the file, even if they haven't been modified.
    pub content: String,

    /// If set to true, existing files will be overwritten. If not set and the
    /// file exists, an error will be returned with the content of the
    /// existing file.
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub overwrite: bool,
}

/// Input type for the file search tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FSSearchInput {
    /// The absolute path of the directory or file to search in. If it's a
    /// directory, it will be searched recursively. If it's a file path,
    /// only that specific file will be searched.
    pub path: String,

    /// The regular expression pattern to search for in file contents. Uses Rust
    /// regex syntax. If not provided, only file name matching will be
    /// performed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,

    /// Glob pattern to filter files (e.g., '*.ts' for TypeScript files).
    /// If not provided, it will search all files (*).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_pattern: Option<String>,
}

/// Input type for the file remove tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FSRemoveInput {
    /// The path of the file to remove (absolute path required)
    pub path: String,
}

/// Operation types that can be performed on matched text
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PatchOperation {
    /// Prepend content before the matched text
    Prepend,

    /// Append content after the matched text
    Append,

    /// Replace the matched text with new content
    Replace,

    /// Swap the matched text with another text (search for the second text and
    /// swap them)
    Swap,
}

/// Input type for the file patch tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FSPatchInput {
    /// The path to the file to modify
    pub path: String,

    /// The text to search for in the source. If empty, operation applies to the
    /// end of the file.
    pub search: String,

    /// The operation to perform on the matched text. Possible options are only
    /// 'prepend', 'append', 'replace', and 'swap'.
    pub operation: PatchOperation,

    /// The content to use for the operation (replacement text, text to
    /// prepend/append, or target text for swap operations)
    pub content: String,
}

/// Input type for the file undo tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FSUndoInput {
    /// The absolute path of the file to revert to its previous state.
    pub path: String,
}

/// Input type for the shell command tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShellInput {
    /// The shell command to execute.
    pub command: String,

    /// The working directory where the command should be executed.
    pub cwd: PathBuf,

    /// Whether to preserve ANSI escape codes in the output.
    /// If true, ANSI escape codes will be preserved in the output.
    /// If false (default), ANSI escape codes will be stripped from the output.
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub keep_ansi: bool,
}

/// Input type for the net fetch tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NetFetchInput {
    /// URL to fetch
    pub url: String,

    /// Get raw content without any markdown conversion (default: false)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<bool>,
}

/// Input type for the attempt completion tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AttemptCompletionInput {
    /// The result of the task. Formulate this result in a way that is final and
    /// does not require further input from the user. Don't end your result
    /// with questions or offers for further assistance.
    pub result: String,
}

/// Helper function to check if a value equals its default value
fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}
