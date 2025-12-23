#![allow(clippy::enum_variant_names)]
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use convert_case::{Case, Casing};
use derive_more::From;
use eserde::Deserialize;
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use schemars::schema::RootSchema;
use serde::Serialize;
use serde_json::Map;
use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, Display, EnumDiscriminants, EnumIter};

use crate::{ToolCallArguments, ToolCallFull, ToolDefinition, ToolDescription, ToolName};

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
#[strum_discriminants(derive(Display, Serialize, Deserialize, Hash))]
#[strum_discriminants(serde(rename_all = "snake_case"))]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
#[strum_discriminants(name(ToolKind))]
#[strum(serialize_all = "snake_case")]
pub enum ToolCatalog {
    Read(FSRead),
    ReadImage(ReadImage),
    Write(FSWrite),
    FsSearch(FSSearch),
    SemSearch(SemanticSearch),
    Remove(FSRemove),
    Patch(FSPatch),
    Undo(FSUndo),
    Shell(Shell),
    Fetch(NetFetch),
    Followup(Followup),
    Plan(PlanCreate),
    Skill(SkillFetch),
}

/// Input structure for agent tool calls. This serves as the generic schema
/// for dynamically registered agent tools, allowing users to specify tasks
/// for specific agents.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct AgentInput {
    /// A list of clear and detailed descriptions of the tasks to be performed
    /// by the agent in parallel. Provide sufficient context and specific
    /// requirements to enable the agent to understand and execute the work
    /// accurately.
    pub tasks: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Reads file contents from the specified absolute path. Ideal for analyzing
/// code, configuration files, documentation, or textual data. Returns the
/// content as a string with line number prefixes by default. For files larger
/// than 2,000 lines, the tool automatically returns only the first 2,000 lines.
/// You should always rely on this default behavior and avoid specifying custom
/// ranges unless absolutely necessary. If needed, specify a range with the
/// start_line and end_line parameters, ensuring the total range does not exceed
/// 2,000 lines. Specifying a range exceeding this limit will result in an
/// error. Binary files are automatically detected and rejected.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSRead {
    /// The path of the file to read, always provide absolute paths.
    pub path: String,

    /// Optional start position in lines (1-based). If provided, reading
    /// will start from this line position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i32>,

    /// If true, prefixes each line with its line index (starting at 1).
    /// Defaults to true.
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,

    /// Optional end position in lines (inclusive). If provided, reading
    /// will end at this line position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<i32>,
}

/// Reads image files from the file system and returns them in base64-encoded
/// format for vision-capable models. Supports common image formats: JPEG, PNG,
/// WebP, and GIF. The path must be absolute and point to an existing file. Use
/// this tool when you need to process, analyze, or display images with vision
/// models. Do NOT use this for text files - use the `read` tool instead. Do NOT
/// use for other binary files like PDFs, videos, or archives. The tool will
/// fail if the file doesn't exist or if the format is unsupported. Returns the
/// image content encoded in base64 format ready for vision model consumption.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct ReadImage {
    /// The absolute path to the image file (e.g., /home/user/image.png).
    /// Relative paths are not supported. The file must exist and be readable.
    pub path: String,
}

/// Use it to create a new file at a specified path with the provided content.
///
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
}

/// A paired query and use_case for semantic search. Each query must have a
/// corresponding use_case for document reranking.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SearchQuery {
    /// Describe WHAT the code does or its purpose. Include domain-specific
    /// terms and technical context. Good: "retry mechanism with exponential
    /// backoff", "streaming responses from LLM API", "OAuth token refresh
    /// flow". Bad: generic terms like "retry" or "auth" without context. Think
    /// about the behavior and functionality you're looking for.
    pub query: String,

    /// A short natural-language description of what you are trying to find.
    /// This is the query used for document reranking. The query MUST:
    /// - express a single, focused information need
    /// - describe exactly what the agent is searching for
    /// - should not be the query verbatim
    /// - be concise (1–2 sentences)
    ///
    /// Examples:
    /// - "Why is `select_model()` returning a Pin<Box<Result>> in Rust?"
    /// - "How to fix error E0277 for the ? operator on a pinned boxed result?"
    /// - "Steps to run Diesel migrations in Rust without exposing the DB."
    /// - "How to design a clean architecture service layer with typed errors?"
    pub use_case: String,
}

impl SearchQuery {
    /// Creates a new search query with the given query and use_case
    pub fn new(query: impl Into<String>, use_case: impl Into<String>) -> Self {
        Self { query: query.into(), use_case: use_case.into() }
    }
}

/// AI-powered semantic code search. YOUR DEFAULT TOOL for code discovery
/// tasks. Use this when you need to find code locations, understand
/// implementations, or explore functionality - it works with natural language
/// about behavior and concepts, not just keyword matching.
///
/// Start with sem_search when: locating code to modify, understanding how
/// features work, finding patterns/examples, or exploring unfamiliar areas.
/// Understands queries like "authentication flow" (finds login), "retry logic"
/// (finds backoff), "validation" (finds checking/sanitization).
///
/// Returns file:line locations with code context, ranked by relevance. Use
/// multiple varied queries (2-3) for best coverage. For exact string matching
/// (TODO comments, specific function names), use regex search instead.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct SemanticSearch {
    /// List of search queries to execute in parallel. Using multiple queries
    /// (2-3) with varied phrasings significantly improves results - each query
    /// captures different aspects of what you're looking for. Each query pairs
    /// a search term with a use_case for reranking. Example: for
    /// authentication, try "user login verification", "token generation",
    /// "OAuth flow".
    pub queries: Vec<SearchQuery>,

    /// Optional file extension filter (e.g., ".rs", ".ts", ".py"). If provided,
    /// only files with this extension will be included in the search results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_extension: Option<String>,
}

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSRemove {
    /// The path of the file to remove (absolute path required)
    pub path: String,
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
/// prepend, append, replace, replace_all, swap operations. Ideal for precise
/// changes to configs, code, or docs while preserving context. Not suitable for
/// complex refactoring or modifying all pattern occurrences - use `write`
/// instead for complete rewrites and `undo` for undoing the last operation.
/// Fails if search pattern isn't found.\n\nUsage Guidelines:\n-When editing
/// text from Read tool output, ensure you preserve new lines and the exact
/// indentation (tabs/spaces) as it appears AFTER the line number prefix. The
/// line number prefix format is: line number + ':'. Everything
/// after that is the actual file content to match. Never include any part
/// of the line number prefix in the search or content
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSPatch {
    /// The path to the file to modify
    pub path: String,

    /// The text to replace. When skipped the patch operation applies to the
    /// entire content. `Append` adds the new content to the end, `Prepend` adds
    /// it to the beginning, and `Replace` fully overwrites the original
    /// content. `Swap` requires a search target, so without one, it makes no
    /// changes.
    pub search: Option<String>,

    /// The operation to perform on the matched text. Possible options are: -
    /// 'prepend': Add content before the matched text - 'append': Add content
    /// after the matched text - 'replace': Use only for specific, targeted
    /// replacements where you need to modify just the first match. -
    /// 'replace_all': Should be used for renaming variables, functions, types,
    /// or any widespread replacements across the file. This is the recommended
    /// choice for consistent refactoring operations as it ensures all
    /// occurrences are updated. - 'swap': Replace the matched text with another
    /// text (search for the second text and swap them)
    pub operation: PatchOperation,

    /// The text to replace it with (must be different from search)
    pub content: String,
}

/// Reverts the most recent file operation (create/modify/delete) on a specific
/// file. Use this tool when you need to recover from incorrect file changes or
/// if a revert is requested by the user.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct FSUndo {
    /// The absolute path of the file to revert to its previous state.
    pub path: String,
}

/// Executes shell commands.
/// The `cwd` parameter sets the working directory for command execution.
/// CRITICAL: Do NOT use `cd` commands in the command string. This is FORBIDDEN.
/// Always use the `cwd` parameter to set the working directory instead. Any use
/// of `cd` in the command is redundant, incorrect, and violates the tool
/// contract. Use for file system interaction, running utilities, installing
/// packages, or executing build commands. Returns complete output including
/// stdout, stderr, and exit code for diagnostic purposes.
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
    /// Environment variable names to pass to command execution (e.g., ["PATH",
    /// "HOME", "USER"]). The system automatically reads the specified
    /// values and applies them during command execution.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<String>>,
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
}

/// Creates a new plan file with the specified name, version, and content. Use
/// this tool to create structured project plans, task breakdowns, or
/// implementation strategies that can be tracked and referenced throughout
/// development sessions.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct PlanCreate {
    /// The name of the plan (will be used in the filename)
    pub plan_name: String,

    /// The version of the plan (e.g., "v1", "v2", "1.0")
    pub version: String,

    /// The content to write to the plan file. This should be the complete
    /// plan content in markdown format.
    pub content: String,
}

/// Fetches detailed information about a specific skill. Use this tool to load
/// skill content and instructions when you need to understand how to perform a
/// specialized task. Skills provide domain-specific knowledge, workflows, and
/// best practices. Only invoke skills that are listed in the available skills
/// section. Do not invoke a skill that is already active.
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq)]
pub struct SkillFetch {
    /// The name of the skill to fetch (e.g., "pdf", "code_review")
    pub name: String,
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
}

#[derive(Deserialize, JsonSchema)]
pub struct UndoInput {
    /// The absolute path of the file to revert to its previous state. Must be
    /// the exact path that was previously modified, created, or deleted by
    /// a Forge file operation. If the file was deleted, provide the
    /// original path it had before deletion. The system requires a prior
    /// snapshot for this path.
    pub path: String,
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
}

/// Helper function to check if a value equals its default value
fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}

impl ToolDescription for ToolCatalog {
    fn description(&self) -> String {
        match self {
            ToolCatalog::Patch(v) => v.description(),
            ToolCatalog::Shell(v) => v.description(),
            ToolCatalog::Followup(v) => v.description(),
            ToolCatalog::Fetch(v) => v.description(),
            ToolCatalog::FsSearch(v) => v.description(),
            ToolCatalog::SemSearch(v) => v.description(),
            ToolCatalog::Read(v) => v.description(),
            ToolCatalog::ReadImage(v) => v.description(),
            ToolCatalog::Remove(v) => v.description(),
            ToolCatalog::Undo(v) => v.description(),
            ToolCatalog::Write(v) => v.description(),
            ToolCatalog::Plan(v) => v.description(),
            ToolCatalog::Skill(v) => v.description(),
        }
    }
}
lazy_static::lazy_static! {
    // Cache of all tool names
    static ref FORGE_TOOLS: HashSet<ToolName> = ToolCatalog::iter()
        .map(ToolName::new)
        .collect();
}

impl ToolCatalog {
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
            ToolCatalog::Patch(_) => r#gen.into_root_schema_for::<FSPatch>(),
            ToolCatalog::Shell(_) => r#gen.into_root_schema_for::<Shell>(),
            ToolCatalog::Followup(_) => r#gen.into_root_schema_for::<Followup>(),
            ToolCatalog::Fetch(_) => r#gen.into_root_schema_for::<NetFetch>(),
            ToolCatalog::FsSearch(_) => r#gen.into_root_schema_for::<FSSearch>(),
            ToolCatalog::SemSearch(_) => r#gen.into_root_schema_for::<SemanticSearch>(),
            ToolCatalog::Read(_) => r#gen.into_root_schema_for::<FSRead>(),
            ToolCatalog::ReadImage(_) => r#gen.into_root_schema_for::<ReadImage>(),
            ToolCatalog::Remove(_) => r#gen.into_root_schema_for::<FSRemove>(),
            ToolCatalog::Undo(_) => r#gen.into_root_schema_for::<FSUndo>(),
            ToolCatalog::Write(_) => r#gen.into_root_schema_for::<FSWrite>(),
            ToolCatalog::Plan(_) => r#gen.into_root_schema_for::<PlanCreate>(),
            ToolCatalog::Skill(_) => r#gen.into_root_schema_for::<SkillFetch>(),
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
    pub fn should_yield(tool_name: &ToolName) -> bool {
        // Tools that convey that the execution should yield
        [ToolKind::Followup]
            .iter()
            .any(|v| v.to_string().to_case(Case::Snake).eq(tool_name.as_str()))
    }

    /// Convert a tool input to its corresponding domain operation for policy
    /// checking. Returns None for tools that don't require permission
    /// checks.
    pub fn to_policy_operation(
        &self,
        cwd: PathBuf,
    ) -> Option<crate::policies::PermissionOperation> {
        let cwd_path = cwd.clone();
        let display_path_for = |path: &str| {
            format!(
                "`{}`",
                format_display_path(Path::new(path), cwd_path.as_path())
            )
        };

        match self {
            ToolCatalog::Read(input) => Some(crate::policies::PermissionOperation::Read {
                path: std::path::PathBuf::from(&input.path),
                cwd,
                message: format!("Read file: {}", display_path_for(&input.path)),
            }),
            ToolCatalog::ReadImage(input) => Some(crate::policies::PermissionOperation::Read {
                path: std::path::PathBuf::from(&input.path),
                cwd,
                message: format!("Image file: {}", display_path_for(&input.path)),
            }),

            ToolCatalog::Write(input) => Some(crate::policies::PermissionOperation::Write {
                path: std::path::PathBuf::from(&input.path),
                cwd,
                message: format!("Create/overwrite file: {}", display_path_for(&input.path)),
            }),
            ToolCatalog::FsSearch(input) => {
                let base_message = format!(
                    "Search in directory/file: {}",
                    display_path_for(&input.path)
                );
                let message = match (&input.regex, &input.file_pattern) {
                    (Some(regex), Some(pattern)) => {
                        format!("{base_message} for pattern: '{regex}' in '{pattern}' files")
                    }
                    (Some(regex), None) => {
                        format!("{base_message} for pattern: {regex}")
                    }
                    (None, Some(pattern)) => {
                        format!("{base_message} in '{pattern}' files")
                    }
                    (None, None) => base_message,
                };
                Some(crate::policies::PermissionOperation::Read {
                    path: std::path::PathBuf::from(&input.path),
                    cwd,
                    message,
                })
            }
            ToolCatalog::Remove(input) => Some(crate::policies::PermissionOperation::Write {
                path: std::path::PathBuf::from(&input.path),
                cwd,
                message: format!("Remove file: {}", display_path_for(&input.path)),
            }),
            ToolCatalog::Patch(input) => Some(crate::policies::PermissionOperation::Write {
                path: std::path::PathBuf::from(&input.path),
                cwd,
                message: format!("Modify file: {}", display_path_for(&input.path)),
            }),
            ToolCatalog::Shell(input) => Some(crate::policies::PermissionOperation::Execute {
                command: input.command.clone(),
                cwd,
            }),
            ToolCatalog::Fetch(input) => Some(crate::policies::PermissionOperation::Fetch {
                url: input.url.clone(),
                cwd,
                message: format!("Fetch content from URL: {}", input.url),
            }),
            // Operations that don't require permission checks
            ToolCatalog::SemSearch(_)
            | ToolCatalog::Undo(_)
            | ToolCatalog::Followup(_)
            | ToolCatalog::Plan(_)
            | ToolCatalog::Skill(_) => None,
        }
    }

    /// Creates a Read tool call with the specified path
    pub fn tool_call_read(path: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Read(FSRead {
            path: path.to_string(),
            ..Default::default()
        }))
    }

    /// Creates a ReadImage tool call with the specified path
    pub fn tool_call_read_image(path: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::ReadImage(ReadImage { path: path.to_string() }))
    }

    /// Creates a Write tool call with the specified path and content
    pub fn tool_call_write(path: &str, content: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Write(FSWrite {
            path: path.to_string(),
            content: content.to_string(),
            ..Default::default()
        }))
    }

    /// Creates a Patch tool call with the specified parameters
    pub fn tool_call_patch(
        path: &str,
        content: &str,
        operation: PatchOperation,
        search: Option<&str>,
    ) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Patch(FSPatch {
            path: path.to_string(),
            search: search.map(|s| s.to_string()),
            operation,
            content: content.to_string(),
        }))
    }

    /// Creates a Remove tool call with the specified path
    pub fn tool_call_remove(path: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Remove(FSRemove { path: path.to_string() }))
    }

    /// Creates a Shell tool call with the specified command and working
    /// directory
    pub fn tool_call_shell(command: &str, cwd: impl Into<PathBuf>) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Shell(Shell {
            command: command.to_string(),
            cwd: cwd.into(),
            ..Default::default()
        }))
    }

    /// Creates a Search tool call with the specified path and regex pattern
    pub fn tool_call_search(path: &str, regex: Option<&str>) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::FsSearch(FSSearch {
            path: path.to_string(),
            regex: regex.map(|r| r.to_string()),
            ..Default::default()
        }))
    }

    /// Creates a Semantic Search tool call with the specified queries
    pub fn tool_call_semantic_search(
        queries: Vec<SearchQuery>,
        file_ext: Option<String>,
    ) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::SemSearch(SemanticSearch {
            queries,
            file_extension: file_ext,
        }))
    }

    /// Creates an Undo tool call with the specified path
    pub fn tool_call_undo(path: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Undo(FSUndo { path: path.to_string() }))
    }

    /// Creates a Fetch tool call with the specified url
    pub fn tool_call_fetch(url: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Fetch(NetFetch {
            url: url.to_string(),
            ..Default::default()
        }))
    }

    /// Creates a Followup tool call with the specified question
    pub fn tool_call_followup(question: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Followup(Followup {
            question: question.to_string(),
            ..Default::default()
        }))
    }

    /// Creates a Plan tool call with the specified plan name, version, and
    /// content
    pub fn tool_call_plan(plan_name: &str, version: &str, content: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Plan(PlanCreate {
            plan_name: plan_name.to_string(),
            version: version.to_string(),
            content: content.to_string(),
        }))
    }

    /// Creates a Skill tool call with the specified skill name
    pub fn tool_call_skill(skill_name: &str) -> ToolCallFull {
        ToolCallFull::from(ToolCatalog::Skill(SkillFetch {
            name: skill_name.to_string(),
        }))
    }

    /// Identifies the kind of the built-in Tools
    pub fn kind(&self) -> ToolKind {
        self.clone().into()
    }
}

fn format_display_path(path: &Path, cwd: &Path) -> String {
    // Try to create a relative path for display if possible
    let display_path = if path.starts_with(cwd) {
        match path.strip_prefix(cwd) {
            Ok(rel_path) => rel_path.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    } else {
        path.display().to_string()
    };

    if display_path.is_empty() {
        ".".to_string()
    } else {
        display_path
    }
}

impl TryFrom<ToolCallFull> for ToolCatalog {
    type Error = crate::Error;

    fn try_from(value: ToolCallFull) -> Result<Self, Self::Error> {
        let mut map = Map::new();
        map.insert("name".into(), value.name.as_str().into());
        map.insert("arguments".into(), value.arguments.parse()?);

        serde_json::from_value(serde_json::Value::Object(map))
            .map_err(|error| crate::Error::AgentCallArgument { error })
    }
}

impl ToolKind {
    pub fn name(&self) -> ToolName {
        ToolName::new(self.to_string().to_case(Case::Snake))
    }

    // TODO: This is an extremely slow operation
    pub fn definition(&self) -> ToolDefinition {
        ToolCatalog::iter()
            .find(|tool| tool.definition().name == self.name())
            .map(|tool| tool.definition())
            .expect("Forge tool definition not found")
    }
}

impl TryFrom<&ToolCallFull> for AgentInput {
    type Error = crate::Error;
    fn try_from(value: &ToolCallFull) -> Result<Self, Self::Error> {
        let value = value.arguments.parse()?;
        serde_json::from_value(value).map_err(|error| crate::Error::AgentCallArgument { error })
    }
}

impl From<ToolCatalog> for ToolCallFull {
    fn from(tool: ToolCatalog) -> Self {
        let name = ToolName::new(tool.to_string());
        // Serialize the tool to get the tagged enum structure
        let value = serde_json::to_value(&tool).expect("Failed to serialize tool");

        // Extract just the "arguments" part from the tagged enum
        let arguments = if let Some(args) = value.get("arguments") {
            ToolCallArguments::from(args.clone())
        } else {
            ToolCallArguments::default()
        };

        ToolCallFull { name, call_id: None, arguments }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use strum::IntoEnumIterator;

    use crate::{ToolCatalog, ToolKind, ToolName};

    #[test]
    fn test_tool_definition() {
        let actual = ToolKind::Remove.name();
        let expected = ToolName::new("remove");
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_tool_definition_json() {
        let tools = ToolCatalog::iter()
            .map(|tool| {
                let definition = tool.definition().input_schema;
                serde_json::to_string_pretty(&definition)
                    .expect("Failed to serialize tool definition to JSON")
            })
            .collect::<Vec<_>>()
            .join("\n");

        insta::assert_snapshot!(tools);
    }

    #[test]
    fn test_fs_search_message_with_regex() {
        use std::path::PathBuf;

        use crate::FSSearch;
        use crate::policies::PermissionOperation;

        let search_with_regex = ToolCatalog::FsSearch(FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("fn main".to_string()),
            start_index: None,
            max_search_lines: None,
            file_pattern: None,
        });

        let operation = search_with_regex
            .to_policy_operation(PathBuf::from("/test/cwd"))
            .unwrap();

        match operation {
            PermissionOperation::Read { message, .. } => {
                assert_eq!(
                    message,
                    "Search in directory/file: `/home/user/project` for pattern: fn main"
                );
            }
            _ => panic!("Expected Read operation"),
        }
    }

    #[test]
    fn test_fs_search_message_without_regex() {
        use std::path::PathBuf;

        use crate::FSSearch;
        use crate::policies::PermissionOperation;

        let search_without_regex = ToolCatalog::FsSearch(FSSearch {
            path: "/home/user/project".to_string(),
            regex: None,
            start_index: None,
            max_search_lines: None,
            file_pattern: None,
        });

        let operation = search_without_regex
            .to_policy_operation(PathBuf::from("/test/cwd"))
            .unwrap();

        match operation {
            PermissionOperation::Read { message, .. } => {
                assert_eq!(message, "Search in directory/file: `/home/user/project`");
            }
            _ => panic!("Expected Read operation"),
        }
    }

    #[test]
    fn test_fs_search_message_with_file_pattern_only() {
        use std::path::PathBuf;

        use crate::FSSearch;
        use crate::policies::PermissionOperation;

        let search_with_pattern = ToolCatalog::FsSearch(FSSearch {
            path: "/home/user/project".to_string(),
            regex: None,
            start_index: None,
            max_search_lines: None,
            file_pattern: Some("*.rs".to_string()),
        });

        let operation = search_with_pattern
            .to_policy_operation(PathBuf::from("/test/cwd"))
            .unwrap();

        match operation {
            PermissionOperation::Read { message, .. } => {
                assert_eq!(
                    message,
                    "Search in directory/file: `/home/user/project` in '*.rs' files"
                );
            }
            _ => panic!("Expected Read operation"),
        }
    }

    #[test]
    fn test_fs_search_message_with_regex_and_file_pattern() {
        use std::path::PathBuf;

        use crate::FSSearch;
        use crate::policies::PermissionOperation;

        let search_with_both = ToolCatalog::FsSearch(FSSearch {
            path: "/home/user/project".to_string(),
            regex: Some("fn main".to_string()),
            start_index: None,
            max_search_lines: None,
            file_pattern: Some("*.rs".to_string()),
        });

        let operation = search_with_both
            .to_policy_operation(PathBuf::from("/test/cwd"))
            .unwrap();

        match operation {
            PermissionOperation::Read { message, .. } => {
                assert_eq!(
                    message,
                    "Search in directory/file: `/home/user/project` for pattern: 'fn main' in '*.rs' files"
                );
            }
            _ => panic!("Expected Read operation"),
        }
    }
}
