use std::path::PathBuf;

use forge_tool_macros::ToolDescription;
use schemars::schema::RootSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ToolDefinition, ToolDescription};

/// Enum representing all possible tool input types.
///
/// This enum contains variants for each type of input that can be passed to
/// tools in the application. Each variant corresponds to the input type for a
/// specific tool.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    JsonSchema,
    derive_more::From,
    strum_macros::EnumIter,
    strum_macros::Display,
)]
#[serde(tag = "tool", content = "args")]
pub enum Tools {
    /// Input for the file read tool
    #[serde(rename = "forge_tool_fs_read")]
    FSRead(FSRead),

    /// Input for the file write tool
    #[serde(rename = "forge_tool_fs_create")]
    FSWrite(FSWrite),

    /// Input for the file search tool
    #[serde(rename = "forge_tool_fs_search")]
    FSSearch(FSSearch),

    /// Input for the file remove tool
    #[serde(rename = "forge_tool_fs_remove")]
    FSRemove(FSRemove),

    /// Input for the file patch tool
    #[serde(rename = "forge_tool_fs_patch")]
    FSPatch(FSPatch),

    /// Input for the file undo tool
    #[serde(rename = "forge_tool_fs_undo")]
    FSUndo(FSUndo),

    /// Input for the shell command tool
    #[serde(rename = "forge_tool_process_shell")]
    Shell(Shell),

    /// Input for the net fetch tool
    #[serde(rename = "forge_tool_net_fetch")]
    NetFetch(NetFetch),

    /// Input for the followup tool
    #[serde(rename = "forge_tool_followup")]
    Followup(Followup),

    /// Input for the completion tool
    #[serde(rename = "forge_tool_attempt_completion")]
    AttemptCompletion(AttemptCompletion),
}

/// Reads file contents from the specified absolute path. Ideal for analyzing
/// code, configuration files, documentation, or textual data. Automatically
/// extracts text from PDF and DOCX files, preserving the original formatting.
/// Returns the content as a string. For files larger than 2,000 lines,
/// the tool automatically returns only the first 2,000 lines. You should
/// always rely on this default behavior and avoid specifying custom ranges
/// unless absolutely necessary. If needed, specify a range with the start_line
/// and end_line parameters, ensuring the total range does not exceed 2,000
/// lines. Specifying a range exceeding this limit will result in an error.
/// Binary files are automatically detected and rejected.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct FSRead {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,

    /// Optional start position in lines (1-based). If provided, reading
    /// will start from this line position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,

    /// Optional end position in lines (inclusive). If provided, reading
    /// will end at this line position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u64>,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Use it to create a new file at a specified path with the provided content.
/// Always provide absolute paths for file locations. The tool
/// automatically handles the creation of any missing intermediary directories
/// in the specified path.
/// IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the
/// shell tool instead.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct FSWrite {
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
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Recursively searches directories for files by content (regex) and/or name
/// (glob pattern). Provides context-rich results with line numbers for content
/// matches. Two modes: content search (when regex provided) or file finder
/// (when regex omitted). Uses case-insensitive Rust regex syntax. Requires
/// absolute paths. Avoids binary files and excluded directories. Best for code
/// exploration, API usage discovery, configuration settings, or finding
/// patterns across projects. For large pages, returns the first 200
/// lines and stores the complete content in a temporary file for
/// subsequent access.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct FSSearch {
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
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct FSRemove {
    /// The path of the file to remove (absolute path required)
    pub path: String,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Operation types that can be performed on matched text
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PatchOperation {
    /// Prepend content before the matched text
    #[default]
    Prepend,

    /// Append content after the matched text
    Append,

    /// Replace the matched text with new content
    Replace,

    /// Swap the matched text with another text (search for the second text and
    /// swap them)
    Swap,
}

/// Modifies files with targeted text operations on matched patterns. Supports
/// prepend, append, replace, swap, delete operations on first pattern
/// occurrence. Ideal for precise changes to configs, code, or docs while
/// preserving context. Not suitable for complex refactoring or modifying all
/// pattern occurrences - use forge_tool_fs_create instead for complete
/// rewrites and forge_tool_fs_undo for undoing the last operation. Fails if
/// search pattern isn't found.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct FSPatch {
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

    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Reverts the most recent file operation (create/modify/delete) on a specific
/// file. Use this tool when you need to recover from incorrect file changes or
/// if a revert is requested by the user.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct FSUndo {
    /// The absolute path of the file to revert to its previous state.
    pub path: String,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Executes shell commands with safety measures using restricted bash (rbash).
/// Prevents potentially harmful operations like absolute path execution and
/// directory changes. Use for file system interaction, running utilities,
/// installing packages, or executing build commands. For operations requiring
/// unrestricted access, advise users to run forge CLI with '-u' flag. Returns
/// complete output including stdout, stderr, and exit code for diagnostic
/// purposes.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct Shell {
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

    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Input type for the net fetch tool
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct NetFetch {
    /// URL to fetch
    pub url: String,

    /// Get raw content without any markdown conversion (default: false)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<bool>,

    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Use this tool when you encounter ambiguities, need clarification, or require
/// more details to proceed effectively. Use this tool judiciously to maintain a
/// balance between gathering necessary information and avoiding excessive
/// back-and-forth.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct Followup {
    /// Question to ask the user
    pub question: String,

    /// If true, allows selecting multiple options; if false (default), only one
    /// option can be selected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,

    /// First option to choose from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option1: Option<String>,

    /// Second option to choose from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option2: Option<String>,

    /// Third option to choose from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option3: Option<String>,

    /// Fourth option to choose from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option4: Option<String>,

    /// Fifth option to choose from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option5: Option<String>,

    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// After each tool use, the user will respond with the result of
/// that tool use, i.e. if it succeeded or failed, along with any reasons for
/// failure. Once you've received the results of tool uses and can confirm that
/// the task is complete, use this tool to present the result of your work to
/// the user. The user may respond with feedback if they are not satisfied with
/// the result, which you can use to make improvements and try again.
/// IMPORTANT NOTE: This tool CANNOT be used until you've confirmed from the
/// user that any previous tool uses were successful. Failure to do so will
/// result in code corruption and system failure. Before using this tool, you
/// must ask yourself in <forge_thinking></forge_thinking> tags if you've
/// confirmed from the user that any previous tool uses were successful. If not,
/// then DO NOT use this tool.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription)]
pub struct AttemptCompletion {
    /// The result of the task. Formulate this result in a way that is final and
    /// does not require further input from the user. Don't end your result with
    /// questions or offers for further assistance.
    pub result: String,

    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

fn default_raw() -> Option<bool> {
    Some(false)
}

/// Retrieves content from URLs as markdown or raw text. Enables access to
/// current online information including websites, APIs and documentation. Use
/// for obtaining up-to-date information beyond training data, verifying facts,
/// or retrieving specific online content. Handles HTTP/HTTPS and converts HTML
/// to readable markdown by default. Cannot access private/restricted resources
/// requiring authentication. Respects robots.txt and may be blocked by
/// anti-scraping measures. For large pages, returns the first 40,000 characters
/// and stores the complete content in a temporary file for subsequent access.
#[derive(Default, Deserialize, JsonSchema, ToolDescription)]
pub struct FetchInput {
    /// URL to fetch
    pub url: String,
    /// Get raw content without any markdown conversion (default: false)
    #[serde(default = "default_raw")]
    pub raw: Option<bool>,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}
/// Request to list files and directories within the specified directory. If
/// recursive is true, it will list all files and directories recursively. If
/// recursive is false or not provided, it will only list the top-level
/// contents. The path must be absolute. Do not use this tool to confirm the
/// existence of files you may have created, as the user will let you know if
/// the files were created successfully or not.
#[derive(Default, Deserialize, JsonSchema, ToolDescription)]
pub struct FSListInput {
    /// The path of the directory to list contents for (absolute path required)
    pub path: String,
    /// Whether to list files recursively. Use true for recursive listing, false
    /// or omit for top-level only.
    pub recursive: Option<bool>,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Request to retrieve detailed metadata about a file or directory at the
/// specified path. Returns comprehensive information including size, creation
/// time, last modified time, permissions, and type. Path must be absolute. Use
/// this when you need to understand file characteristics without reading the
/// actual content.
#[derive(Default, Deserialize, JsonSchema, ToolDescription)]
pub struct FSFileInfoInput {
    /// The path of the file or directory to inspect (absolute path required)
    pub path: String,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UndoInput {
    /// The absolute path of the file to revert to its previous state. Must be
    /// the exact path that was previously modified, created, or deleted by
    /// a Forge file operation. If the file was deleted, provide the
    /// original path it had before deletion. The system requires a prior
    /// snapshot for this path.
    pub path: String,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Input for the select tool
#[derive(Deserialize, JsonSchema)]
pub struct SelectInput {
    /// Question to ask the user
    pub question: String,

    /// First option to choose from
    pub option1: Option<String>,

    /// Second option to choose from
    pub option2: Option<String>,

    /// Third option to choose from
    pub option3: Option<String>,

    /// Fourth option to choose from
    pub option4: Option<String>,

    /// Fifth option to choose from
    pub option5: Option<String>,

    /// If true, allows selecting multiple options; if false (default), only one
    /// option can be selected
    #[schemars(default)]
    pub multiple: Option<bool>,
    /// One sentence explanation as to why this tool is being used, and how it
    /// contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Helper function to check if a value equals its default value
fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

impl ToolDescription for Tools {
    fn description(&self) -> String {
        match self {
            Tools::FSPatch(v) => v.description(),
            Tools::Shell(v) => v.description(),
            Tools::Followup(v) => v.description(),
            Tools::NetFetch(v) => v.description(),
            Tools::AttemptCompletion(v) => v.description(),
            Tools::FSSearch(v) => v.description(),
            Tools::FSRead(v) => v.description(),
            Tools::FSRemove(v) => v.description(),
            Tools::FSUndo(v) => v.description(),
            Tools::FSWrite(v) => v.description(),
        }
    }
}

impl Tools {
    pub fn schema(&self) -> RootSchema {
        match self {
            Tools::FSPatch(_) => schemars::schema_for!(FSPatch),
            Tools::Shell(_) => schemars::schema_for!(Shell),
            Tools::Followup(_) => schemars::schema_for!(Followup),
            Tools::NetFetch(_) => schemars::schema_for!(NetFetch),
            Tools::AttemptCompletion(_) => schemars::schema_for!(AttemptCompletion),
            Tools::FSSearch(_) => schemars::schema_for!(FSSearch),
            Tools::FSRead(_) => schemars::schema_for!(FSRead),
            Tools::FSRemove(_) => schemars::schema_for!(FSRemove),
            Tools::FSUndo(_) => schemars::schema_for!(FSUndo),
            Tools::FSWrite(_) => schemars::schema_for!(FSWrite),
        }
    }

    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self)
            .description(self.description())
            .input_schema(self.schema())
    }
}
