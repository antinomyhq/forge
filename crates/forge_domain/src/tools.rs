use crate::ToolDescription;
use forge_tool_macros::ToolDescription;

/// Modifies files with targeted text operations on matched patterns. Supports
/// prepend, append, replace, swap, delete operations on first pattern
/// occurrence. Ideal for precise changes to configs, code, or docs while
/// preserving context. Not suitable for complex refactoring or modifying all
/// pattern occurrences - use forge_tool_fs_create instead for complete
/// rewrites and forge_tool_fs_undo for undoing the last operation. Fails if
/// search pattern isn't found.
#[derive(Default, ToolDescription)]
pub struct ApplyPatchJson;

/// Executes shell commands with safety measures using restricted bash (rbash).
/// Prevents potentially harmful operations like absolute path execution and
/// directory changes. Use for file system interaction, running utilities,
/// installing packages, or executing build commands. For operations requiring
/// unrestricted access, advise users to run forge CLI with '-u' flag. Returns
/// complete output including stdout, stderr, and exit code for diagnostic
/// purposes.
#[derive(Default, ToolDescription)]
pub struct Shell;

/// Use this tool when you encounter ambiguities, need clarification, or require
/// more details to proceed effectively. Use this tool judiciously to maintain a
/// balance between gathering necessary information and avoiding excessive
/// back-and-forth.
#[derive(Default, ToolDescription)]
pub struct Followup;

/// Retrieves content from URLs as markdown or raw text. Enables access to
/// current online information including websites, APIs and documentation. Use
/// for obtaining up-to-date information beyond training data, verifying facts,
/// or retrieving specific online content. Handles HTTP/HTTPS and converts HTML
/// to readable markdown by default. Cannot access private/restricted resources
/// requiring authentication. Respects robots.txt and may be blocked by
/// anti-scraping measures. For large pages, returns the first 40,000 characters
/// and stores the complete content in a temporary file for subsequent access.
#[derive(Default, ToolDescription)]
pub struct Fetch;

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
#[derive(Default, ToolDescription)]
pub struct Completion;

/// Request to retrieve detailed metadata about a file or directory at the
/// specified path. Returns comprehensive information including size, creation
/// time, last modified time, permissions, and type. Path must be absolute. Use
/// this when you need to understand file characteristics without reading the
/// actual content.
#[derive(Default, ToolDescription)]
pub struct FSFileInfo;

/// Recursively searches directories for files by content (regex) and/or name
/// (glob pattern). Provides context-rich results with line numbers for content
/// matches. Two modes: content search (when regex provided) or file finder
/// (when regex omitted). Uses case-insensitive Rust regex syntax. Requires
/// absolute paths. Avoids binary files and excluded directories. Best for code
/// exploration, API usage discovery, configuration settings, or finding
/// patterns across projects. For large pages, returns the first 200
/// lines and stores the complete content in a temporary file for
/// subsequent access.
#[derive(Default, ToolDescription)]
pub struct FSFind;

/// Request to list files and directories within the specified directory. If
/// recursive is true, it will list all files and directories recursively. If
/// recursive is false or not provided, it will only list the top-level
/// contents. The path must be absolute. Do not use this tool to confirm the
/// existence of files you may have created, as the user will let you know if
/// the files were created successfully or not.
#[derive(Default, ToolDescription)]
pub struct FSList;

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
#[derive(Default, ToolDescription)]
pub struct FSRead;

/// Request to remove a file at the specified path. Use this when you need to
/// delete an existing file. The path must be absolute. This operation cannot
/// be undone, so use it carefully.
#[derive(Default, ToolDescription)]
pub struct FSRemove;

/// Reverts the most recent file operation (create/modify/delete) on a specific
/// file. Use this tool when you need to recover from incorrect file changes or
/// if a revert is requested by the user.
#[derive(Default, ToolDescription)]
pub struct FSUndo;

/// Use it to create a new file at a specified path with the provided content.
/// Always provide absolute paths for file locations. The tool
/// automatically handles the creation of any missing intermediary directories
/// in the specified path.
/// IMPORTANT: DO NOT attempt to use this tool to move or rename files, use the
/// shell tool instead.
#[derive(Default, ToolDescription)]
pub struct FsCreate;

#[derive(strum_macros::EnumIter, strum_macros::Display)]
pub enum Tools {
    #[strum(serialize = "forge_tool_fs_patch")]
    ApplyPatchJson(ApplyPatchJson),
    #[strum(serialize = "forge_tool_process_shell")]
    Shell(Shell),
    #[strum(serialize = "forge_tool_followup")]
    Followup(Followup),
    #[strum(serialize = "forge_tool_net_fetch")]
    Fetch(Fetch),
    #[strum(serialize = "forge_tool_attempt_completion")]
    Completion(Completion),
    #[strum(serialize = "forge_tool_fs_info")]
    FSFileInfo(FSFileInfo),
    #[strum(serialize = "forge_tool_fs_search")]
    FSFind(FSFind),
    #[strum(serialize = "forge_tool_fs_list")]
    FSList(FSList),
    #[strum(serialize = "forge_tool_fs_read")]
    FSRead(FSRead),
    #[strum(serialize = "forge_tool_fs_remove")]
    FSRemove(FSRemove),
    #[strum(serialize = "forge_tool_fs_undo")]
    FSUndo(FSUndo),
    #[strum(serialize = "forge_tool_fs_create")]
    FSCreate(FsCreate),
}

impl ToolDescription for Tools {
    fn description(&self) -> String {
        match self {
            Tools::ApplyPatchJson(v) => v.description(),
            Tools::Shell(v) => v.description(),
            Tools::Followup(v) => v.description(),
            Tools::Fetch(v) => v.description(),
            Tools::Completion(v) => v.description(),
            Tools::FSFileInfo(v) => v.description(),
            Tools::FSFind(v) => v.description(),
            Tools::FSList(v) => v.description(),
            Tools::FSRead(v) => v.description(),
            Tools::FSRemove(v) => v.description(),
            Tools::FSUndo(v) => v.description(),
            Tools::FSCreate(v) => v.description(),
        }
    }
}
