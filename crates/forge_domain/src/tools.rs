#![allow(clippy::enum_variant_names)]
use std::collections::HashSet;
use std::path::PathBuf;

use convert_case::{Case, Casing};
use derive_more::From;
use eserde::Deserialize;
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use schemars::schema::RootSchema;
use serde::Serialize;
use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, Display, EnumDiscriminants, EnumIter};

use crate::{
    Status, ToolCallArgumentError, ToolCallFull, ToolDefinition, ToolDescription, ToolName,
};

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
    From,
    EnumIter,
    Display,
    PartialEq,
    EnumDiscriminants,
)]
#[strum_discriminants(derive(Display))]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Tools {
    ForgeToolFsRead(FSRead),
    ForgeToolFsCreate(FSWrite),
    ForgeToolFsSearch(FSSearch),
    ForgeToolFsRemove(FSRemove),
    ForgeToolFsPatch(FSPatch),
    ForgeToolFsUndo(FSUndo),
    ForgeToolProcessShell(Shell),
    ForgeToolNetFetch(NetFetch),
    ForgeToolFollowup(Followup),
    ForgeToolAttemptCompletion(AttemptCompletion),
    ForgeToolTaskListAppend(TaskListAppend),
    ForgeToolTaskListAppendMultiple(TaskListAppendMultiple),
    ForgeToolTaskListUpdate(TaskListUpdate),
    ForgeToolTaskListList(TaskListList),
    ForgeToolTaskListClear(TaskListClear),
}

/// Input structure for agent tool calls. This serves as the generic schema
/// for dynamically registered agent tools, allowing users to specify tasks
/// for specific agents.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct AgentInput {
    /// A clear and detailed description of the task to be performed by the
    /// agent. Provide sufficient context and specific requirements to
    /// enable the agent to understand and execute the work accurately.
    pub task: String,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
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
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSRead {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,

    /// Optional start position in lines (1-based). If provided, reading
    /// will start from this line position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i32>,

    /// Optional end position in lines (inclusive). If provided, reading
    /// will end at this line position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<i32>,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Use it to create a new file at a specified path with the provided content.
/// Always provide absolute paths for file locations. The tool
/// automatically handles the creation of any missing intermediary directories
/// in the specified path.
/// IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the
/// shell tool instead.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
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
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
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
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
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

    /// Starting index for the search results (1-based).
    pub start_index: Option<i32>,

    /// Maximum number of lines to return in the search results.
    pub max_search_lines: Option<i32>,

    /// Glob pattern to filter files (e.g., '*.ts' for TypeScript files).
    /// If not provided, it will search all files (*).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_pattern: Option<String>,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSRemove {
    /// The path of the file to remove (absolute path required)
    pub path: String,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Operation types that can be performed on matched text
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, AsRefStr, EnumIter)]
#[serde(rename_all = "snake_case")]
pub enum PatchOperation {
    /// Prepend content before the matched text
    #[default]
    Prepend,

    /// Append content after the matched text
    Append,

    /// Should be used only when you want to replace the first occurrence.
    /// Use only for specific, targeted replacements where you need to modify
    /// just the first match.
    Replace,

    /// Should be used for renaming variables, functions, types, or any
    /// widespread replacements across the file. This is the recommended
    /// choice for consistent refactoring operations as it ensures all
    /// occurrences are updated.
    ReplaceAll,

    /// Swap the matched text with another text (search for the second text and
    /// swap them)
    Swap,
}

// TODO: do the Blanket impl for all the unit enums
impl JsonSchema for PatchOperation {
    fn schema_name() -> String {
        std::any::type_name::<Self>()
            .split("::")
            .last()
            .unwrap_or("PatchOperation")
            .to_string()
    }

    fn json_schema(_gen: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, Schema, SchemaObject};
        let variants: Vec<serde_json::Value> = Self::iter()
            .map(|variant| variant.as_ref().to_case(Case::Snake).into())
            .collect();
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(variants),
            metadata: Some(Box::new(schemars::schema::Metadata {
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

/// Modifies files with targeted line operations on matched patterns. Supports
/// prepend, append, replace, replace_all, swap, delete
/// operations. Ideal for precise changes to configs, code, or docs while
/// preserving context. Not suitable for complex refactoring or modifying all
/// pattern occurrences - use `forge_tool_fs_create` instead for complete
/// rewrites and `forge_tool_fs_undo` for undoing the last operation. Fails if
/// search pattern isn't found.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSPatch {
    /// The path to the file to modify
    pub path: String,

    /// The exact line to search for in the file. When
    /// skipped the patch operation applies to the entire content. `Append` adds
    /// the new content to the end, `Prepend` adds it to the beginning, and
    /// `Replace` fully overwrites the original content. `Swap` requires a
    /// search target, so without one, it makes no changes.
    pub search: Option<String>,

    /// The operation to perform on the matched text. Possible options are:
    /// - 'prepend': Add content before the matched text
    /// - 'append': Add content after the matched text
    /// - 'replace': Use only for specific, targeted replacements where you need
    ///   to modify just the first match.
    /// - 'replace_all': Should be used for renaming variables, functions,
    ///   types, or any widespread replacements across the file. This is the
    ///   recommended choice for consistent refactoring operations as it ensures
    ///   all occurrences are updated.
    /// - 'swap': Replace the matched text with another text (search for the
    ///   second text and swap them)
    pub operation: PatchOperation,

    /// The content to use for the operation (replacement text, line to
    /// prepend/append, or target line for swap operations)
    pub content: String,

    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Reverts the most recent file operation (create/modify/delete) on a specific
/// file. Use this tool when you need to recover from incorrect file changes or
/// if a revert is requested by the user.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSUndo {
    /// The absolute path of the file to revert to its previous state.
    pub path: String,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
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
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
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

    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Input type for the net fetch tool
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct NetFetch {
    /// URL to fetch
    pub url: String,

    /// Get raw content without any markdown conversion (default: false)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<bool>,

    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Use this tool when you encounter ambiguities, need clarification, or require
/// more details to proceed effectively. Use this tool judiciously to maintain a
/// balance between gathering necessary information and avoiding excessive
/// back-and-forth.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
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

    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
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
/// must ask yourself if you've confirmed from the user that any previous tool
/// uses were successful. If not, then DO NOT use this tool.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct AttemptCompletion {
    /// The result of the task. Formulate this result in a way that is final and
    /// does not require further input from the user. Don't end your result with
    /// questions or offers for further assistance.
    pub result: String,
}

/// Add a new task to the end of the task list. Tasks are stored in conversation
/// state and persist across agent interactions. Use this tool to add individual
/// work items that need to be tracked during development sessions. Task IDs are
/// auto-generated integers starting from 1.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct TaskListAppend {
    /// The task description to add to the list
    pub task: String,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Add multiple new tasks to the end of the task list. Tasks are stored in
/// conversation state and persist across agent interactions. Use this tool to
/// add several work items at once during development sessions. Task IDs are
/// auto-generated integers starting from 1.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct TaskListAppendMultiple {
    /// The list of task descriptions to add
    pub tasks: Vec<String>,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Update the status of a specific task in the task list. Use this when a
/// task's status changes (e.g., from Pending to InProgress, InProgress to Done,
/// etc.). The task will remain in the list but with an updated status.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct TaskListUpdate {
    /// The ID of the task to update
    pub task_id: i32,
    /// The new status for the task
    pub status: Status,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Display the current task list with statistics. Shows all tasks with their
/// IDs, descriptions, and status (PENDING, IN_PROGRESS, DONE), along with
/// summary statistics. Use this tool to review current work items and track
/// progress through development sessions.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct TaskListList {
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Remove all tasks from the task list. This operation cannot be undone and
/// will reset the task ID counter to 1. Use this tool when you want to start
/// fresh with a clean task list.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct TaskListClear {
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
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
#[derive(Default, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FetchInput {
    /// URL to fetch
    pub url: String,
    /// Get raw content without any markdown conversion (default: false)
    #[serde(default = "default_raw")]
    pub raw: Option<bool>,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}
/// Request to list files and directories within the specified directory. If
/// recursive is true, it will list all files and directories recursively. If
/// recursive is false or not provided, it will only list the top-level
/// contents. The path must be absolute. Do not use this tool to confirm the
/// existence of files you may have created, as the user will let you know if
/// the files were created successfully or not.
#[derive(Default, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSListInput {
    /// The path of the directory to list contents for (absolute path required)
    pub path: String,
    /// Whether to list files recursively. Use true for recursive listing, false
    /// or omit for top-level only.
    pub recursive: Option<bool>,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Request to retrieve detailed metadata about a file or directory at the
/// specified path. Returns comprehensive information including size, creation
/// time, last modified time, permissions, and type. Path must be absolute. Use
/// this when you need to understand file characteristics without reading the
/// actual content.
#[derive(Default, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSFileInfoInput {
    /// The path of the file or directory to inspect (absolute path required)
    pub path: String,
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
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
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
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
    /// One sentence explanation as to why this specific tool is being used, and
    /// how it contributes to the goal.
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
            Tools::ForgeToolFsPatch(v) => v.description(),
            Tools::ForgeToolProcessShell(v) => v.description(),
            Tools::ForgeToolFollowup(v) => v.description(),
            Tools::ForgeToolNetFetch(v) => v.description(),
            Tools::ForgeToolAttemptCompletion(v) => v.description(),
            Tools::ForgeToolFsSearch(v) => v.description(),
            Tools::ForgeToolFsRead(v) => v.description(),
            Tools::ForgeToolFsRemove(v) => v.description(),
            Tools::ForgeToolFsUndo(v) => v.description(),
            Tools::ForgeToolFsCreate(v) => v.description(),
            Tools::ForgeToolTaskListAppend(v) => v.description(),
            Tools::ForgeToolTaskListAppendMultiple(v) => v.description(),
            Tools::ForgeToolTaskListUpdate(v) => v.description(),
            Tools::ForgeToolTaskListList(v) => v.description(),
            Tools::ForgeToolTaskListClear(v) => v.description(),
        }
    }
}
lazy_static::lazy_static! {
    // Cache of all tool names
    static ref FORGE_TOOLS: HashSet<ToolName> = Tools::iter()
        .map(ToolName::new)
        .collect();
}

impl Tools {
    pub fn schema(&self) -> RootSchema {
        use schemars::r#gen::SchemaSettings;
        let r#gen = SchemaSettings::default()
            .with(|s| {
                // incase of null, add nullable property.
                s.option_nullable = true;
                // incase of option type, don't add null in type.
                s.option_add_null_type = false;
                s.meta_schema = None;
                s.inline_subschemas = true;
            })
            .into_generator();
        match self {
            Tools::ForgeToolFsPatch(_) => r#gen.into_root_schema_for::<FSPatch>(),
            Tools::ForgeToolProcessShell(_) => r#gen.into_root_schema_for::<Shell>(),
            Tools::ForgeToolFollowup(_) => r#gen.into_root_schema_for::<Followup>(),
            Tools::ForgeToolNetFetch(_) => r#gen.into_root_schema_for::<NetFetch>(),
            Tools::ForgeToolAttemptCompletion(_) => {
                r#gen.into_root_schema_for::<AttemptCompletion>()
            }
            Tools::ForgeToolFsSearch(_) => r#gen.into_root_schema_for::<FSSearch>(),
            Tools::ForgeToolFsRead(_) => r#gen.into_root_schema_for::<FSRead>(),
            Tools::ForgeToolFsRemove(_) => r#gen.into_root_schema_for::<FSRemove>(),
            Tools::ForgeToolFsUndo(_) => r#gen.into_root_schema_for::<FSUndo>(),
            Tools::ForgeToolFsCreate(_) => r#gen.into_root_schema_for::<FSWrite>(),
            Tools::ForgeToolTaskListAppend(_) => r#gen.into_root_schema_for::<TaskListAppend>(),
            Tools::ForgeToolTaskListAppendMultiple(_) => {
                r#gen.into_root_schema_for::<TaskListAppendMultiple>()
            }
            Tools::ForgeToolTaskListUpdate(_) => r#gen.into_root_schema_for::<TaskListUpdate>(),
            Tools::ForgeToolTaskListList(_) => r#gen.into_root_schema_for::<TaskListList>(),
            Tools::ForgeToolTaskListClear(_) => r#gen.into_root_schema_for::<TaskListClear>(),
        }
    }

    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(self)
            .description(self.description())
            .input_schema(self.schema())
    }
    pub fn contains(tool_name: &ToolName) -> bool {
        FORGE_TOOLS.contains(tool_name)
    }
    pub fn is_complete(tool_name: &ToolName) -> bool {
        // Tools that convey that the execution should yield
        [
            ToolsDiscriminants::ForgeToolFollowup,
            ToolsDiscriminants::ForgeToolAttemptCompletion,
        ]
        .iter()
        .any(|v| v.to_string().to_case(Case::Snake).eq(tool_name.as_str()))
    }
}

impl ToolsDiscriminants {
    pub fn name(&self) -> ToolName {
        ToolName::new(self.to_string().to_case(Case::Snake))
    }

    // TODO: This is an extremely slow operation
    pub fn definition(&self) -> ToolDefinition {
        Tools::iter()
            .find(|tool| tool.definition().name == self.name())
            .map(|tool| tool.definition())
            .expect("Forge tool definition not found")
    }
}

impl TryFrom<ToolCallFull> for Tools {
    type Error = ToolCallArgumentError;

    fn try_from(value: ToolCallFull) -> Result<Self, Self::Error> {
        let arg = if value.arguments.is_null() {
            // Note: If the arguments are null, we use an empty object.
            // This is a workaround for eserde, which doesn't provide
            // detailed error messages when required fields are missing.
            "{}".to_string()
        } else {
            value.arguments.to_string()
        };

        let json_str = format!(r#"{{"name": "{}", "arguments": {}}}"#, value.name, arg);
        eserde::json::from_str(&json_str).map_err(ToolCallArgumentError::from)
    }
}

impl TryFrom<&ToolCallFull> for AgentInput {
    type Error = ToolCallArgumentError;
    fn try_from(value: &ToolCallFull) -> Result<Self, Self::Error> {
        eserde::json::from_str(&value.arguments.to_string()).map_err(ToolCallArgumentError::from)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use strum::IntoEnumIterator;

    use crate::{FSRead, ToolCallFull, ToolName, Tools, ToolsDiscriminants};

    #[test]
    fn foo() {
        let toolcall = ToolCallFull::new(ToolName::new("forge_tool_fs_read")).arguments(json!({
            "path": "/some/path/foo.txt",
        }));

        let actual = Tools::try_from(toolcall).unwrap();
        let expected = Tools::ForgeToolFsRead(FSRead {
            path: "/some/path/foo.txt".to_string(),
            start_line: None,
            end_line: None,
            explanation: None,
        });

        pretty_assertions::assert_eq!(actual, expected);
    }
    #[test]
    fn test_is_complete() {
        let complete_tool = ToolName::new("forge_tool_attempt_completion");
        let incomplete_tool = ToolName::new("forge_tool_fs_read");

        assert!(Tools::is_complete(&complete_tool));
        assert!(!Tools::is_complete(&incomplete_tool));
    }

    #[test]
    fn test_tool_definition() {
        let actual = ToolsDiscriminants::ForgeToolFsRemove.name();
        let expected = ToolName::new("forge_tool_fs_remove");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_tool_definition_json() {
        let tools = Tools::iter()
            .map(|tool| {
                let definition = tool.definition();
                serde_json::to_string_pretty(&definition)
                    .expect("Failed to serialize tool definition to JSON")
            })
            .collect::<Vec<_>>()
            .join("\n");

        insta::assert_snapshot!(tools);
    }

    #[test]
    fn test_tool_deser_failure() {
        let tool_call = ToolCallFull::new("forge_tool_fs_create");
        let result = Tools::try_from(tool_call);
        insta::assert_snapshot!(result.unwrap_err().to_string());
    }

    #[test]
    fn test_correct_deser() {
        let tool_call = ToolCallFull::new("forge_tool_fs_create").arguments(json!({
            "path": "/some/path/foo.txt",
            "content": "Hello, World!",
        }));
        let result = Tools::try_from(tool_call);
        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), Tools::ForgeToolFsCreate(data) if data.path == "/some/path/foo.txt" && data.content == "Hello, World!")
        );
    }
}
