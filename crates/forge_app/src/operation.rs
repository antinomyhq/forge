use std::cmp::min;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use console::strip_ansi_codes;
use derive_setters::Setters;
use forge_display::DiffFormat;
use forge_domain::{
    CodebaseSearchResults, Environment, FSPatch, FSRead, FSRemove, FSSearch, FSUndo, FSWrite,
    FileOperation, LineNumbers, Metrics, NetFetch, PlanCreate, ToolKind,
};
use forge_template::{ElementBuilder, Output, OutputPart};

use crate::truncation::{
    Stderr, Stdout, TruncationMode, truncate_fetch_content, truncate_search_output,
    truncate_shell_output,
};
use crate::utils::{compute_hash, format_display_path};
use crate::{
    FsRemoveOutput, FsUndoOutput, FsWriteOutput, HttpResponse, PatchOutput, PlanCreateOutput,
    ReadOutput, ResponseContext, SearchResult, ShellOutput,
};

#[derive(Debug, Default, Setters)]
#[setters(into, strip_option)]
pub struct TempContentFiles {
    stdout: Option<PathBuf>,
    stderr: Option<PathBuf>,
}

#[derive(Debug, derive_more::From)]
pub enum ToolOperation {
    FsRead {
        input: FSRead,
        output: ReadOutput,
    },
    FsWrite {
        input: FSWrite,
        output: FsWriteOutput,
    },
    FsRemove {
        input: FSRemove,
        output: FsRemoveOutput,
    },
    FsSearch {
        input: FSSearch,
        output: Option<SearchResult>,
    },
    CodebaseSearch {
        output: CodebaseSearchResults,
    },
    FsPatch {
        input: FSPatch,
        output: PatchOutput,
    },
    FsUndo {
        input: FSUndo,
        output: FsUndoOutput,
    },
    NetFetch {
        input: NetFetch,
        output: HttpResponse,
    },
    Shell {
        output: ShellOutput,
    },
    FollowUp {
        output: Option<String>,
    },
    PlanCreate {
        input: PlanCreate,
        output: PlanCreateOutput,
    },
    Skill {
        output: forge_domain::Skill,
    },
}

/// Trait for stream elements that can be converted to XML elements
pub trait StreamElement {
    fn stream_name(&self) -> &'static str;
    fn head_content(&self) -> &str;
    fn tail_content(&self) -> Option<&str>;
    fn total_lines(&self) -> usize;
    fn head_end_line(&self) -> usize;
    fn tail_start_line(&self) -> Option<usize>;
    fn tail_end_line(&self) -> Option<usize>;
}

impl StreamElement for Stdout {
    fn stream_name(&self) -> &'static str {
        "stdout"
    }

    fn head_content(&self) -> &str {
        &self.head
    }

    fn tail_content(&self) -> Option<&str> {
        self.tail.as_deref()
    }

    fn total_lines(&self) -> usize {
        self.total_lines
    }

    fn head_end_line(&self) -> usize {
        self.head_end_line
    }

    fn tail_start_line(&self) -> Option<usize> {
        self.tail_start_line
    }

    fn tail_end_line(&self) -> Option<usize> {
        self.tail_end_line
    }
}

impl StreamElement for Stderr {
    fn stream_name(&self) -> &'static str {
        "stderr"
    }

    fn head_content(&self) -> &str {
        &self.head
    }

    fn tail_content(&self) -> Option<&str> {
        self.tail.as_deref()
    }

    fn total_lines(&self) -> usize {
        self.total_lines
    }

    fn head_end_line(&self) -> usize {
        self.head_end_line
    }

    fn tail_start_line(&self) -> Option<usize> {
        self.tail_start_line
    }

    fn tail_end_line(&self) -> Option<usize> {
        self.tail_end_line
    }
}

/// Helper function to create stdout or stderr elements with consistent
/// structure
fn create_stream_element<T: StreamElement>(
    stream: &T,
    full_output_path: Option<&Path>,
) -> Option<OutputPart> {
    if stream.head_content().is_empty() {
        return None;
    }

    let mut builder = ElementBuilder::new(stream.stream_name())
        .attr("total_lines", stream.total_lines().to_string());

    if let Some(((tail, tail_start), tail_end)) = stream
        .tail_content()
        .zip(stream.tail_start_line())
        .zip(stream.tail_end_line())
    {
        builder = builder
            .child(
                ElementBuilder::new("head")
                    .attr("display_lines", format!("1-{}", stream.head_end_line()))
                    .cdata(stream.head_content())
                    .build(),
            )
            .child(
                ElementBuilder::new("tail")
                    .attr("display_lines", format!("{tail_start}-{tail_end}"))
                    .cdata(tail)
                    .build(),
            );
    } else {
        builder = builder.cdata(stream.head_content());
    }

    if let Some(path) = full_output_path {
        builder = builder.attr("full_output", path.display().to_string());
    }

    Some(builder.build())
}

/// Creates a validation warning element for syntax errors
///
/// # Arguments
/// * `path` - The file path
/// * `errors` - Vector of syntax errors
///
/// Returns an OutputPart containing the formatted warning with all error details
fn create_validation_warning(path: &str, errors: &[forge_domain::SyntaxError]) -> OutputPart {
    let mut builder = ElementBuilder::new("warning")
        .child(ElementBuilder::new("message").text("Syntax validation failed").build())
        .child(ElementBuilder::new("file").attr("path", path).build())
        .child(
            ElementBuilder::new("details")
                .text(format!(
                    "The file was written successfully but contains {} syntax error(s)",
                    errors.len()
                ))
                .build(),
        );

    for error in errors {
        builder = builder.child(
            ElementBuilder::new("error")
                .attr("line", error.line.to_string())
                .attr("column", error.column.to_string())
                .cdata(&error.message)
                .build(),
        );
    }

    builder = builder.child(
        ElementBuilder::new("suggestion")
            .text("Review and fix the syntax issues")
            .build(),
    );

    builder.build()
}

impl ToolOperation {
    pub fn into_tool_output(
        self,
        tool_kind: ToolKind,
        content_files: TempContentFiles,
        env: &Environment,
        metrics: &mut Metrics,
    ) -> forge_domain::ToolOutput {
        let tool_name = tool_kind.name();
        match self {
            ToolOperation::FsRead { input, output } => {
                // Check if content is an image (visual content)
                if let Some(image) = output.content.as_image() {
                    // Track read operations for visual content
                    tracing::info!(
                        path = %input.file_path,
                        tool = %tool_name,
                        "Visual content read (image/PDF)"
                    );
                    *metrics = metrics.clone().insert(
                        input.file_path.clone(),
                        FileOperation::new(tool_kind)
                            .content_hash(Some(output.content_hash.clone())),
                    );

                    return forge_domain::ToolOutput::image(image.clone());
                }

                // Handle text content
                let content = output.content.file_content();
                let content = if input.show_line_numbers {
                    content.to_numbered_from(output.start_line as usize)
                } else {
                    content.to_string()
                };
                let xml = Output::new()
                    .element("file")
                    .attr("path", &input.file_path)
                    .attr(
                        "display_lines",
                        format!("{}-{}", output.start_line, output.end_line),
                    )
                    .attr("total_lines", content.lines().count().to_string())
                    .cdata(content)
                    .done()
                    .render_xml();

                // Track read operations
                tracing::info!(
                    path = %input.file_path,
                    tool = %tool_name,
                    "File read"
                );
                *metrics = metrics.clone().insert(
                    input.file_path.clone(),
                    FileOperation::new(tool_kind).content_hash(Some(output.content_hash.clone())),
                );

                // Generate markdown for ACP display
                let markdown = crate::operation_markdown::format_read(
                    &input.file_path,
                    &output,
                    input.show_line_numbers,
                );

                forge_domain::ToolOutput::paired(xml, markdown)
            }
            ToolOperation::FsWrite { input, output } => {
                let diff_result = DiffFormat::format(
                    output.before.as_ref().unwrap_or(&"".to_string()),
                    &input.content,
                );
                let diff = console::strip_ansi_codes(diff_result.diff()).to_string();

                *metrics = metrics.clone().insert(
                    input.file_path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_added(diff_result.lines_added())
                        .lines_removed(diff_result.lines_removed())
                        .content_hash(Some(output.content_hash.clone())),
                );

                let builder = if output.before.as_ref().is_some() {
                    let mut b = ElementBuilder::new("file_overwritten")
                        .attr("path", &input.file_path)
                        .attr("total_lines", input.content.lines().count().to_string())
                        .child(ElementBuilder::new("file_diff").cdata(diff).build());
                    
                    if !output.errors.is_empty() {
                        b = b.child(create_validation_warning(&input.file_path, &output.errors));
                    }
                    b
                } else {
                    let mut b = ElementBuilder::new("file_created")
                        .attr("path", &input.file_path)
                        .attr("total_lines", input.content.lines().count().to_string());
                    
                    if !output.errors.is_empty() {
                        b = b.child(create_validation_warning(&input.file_path, &output.errors));
                    }
                    b
                };

                let xml = Output::new().part(builder.build()).render_xml();
                
                // Generate markdown for ACP display
                let markdown = crate::operation_markdown::format_write(
                    &input.file_path,
                    &output,
                    &input.content,
                    env.cwd.as_path(),
                );

                forge_domain::ToolOutput::paired(xml, markdown)
            }
            ToolOperation::FsRemove { input, output } => {
                // None since file was removed
                let content_hash = None;

                *metrics = metrics.clone().insert(
                    input.path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_removed(output.content.lines().count() as u64)
                        .content_hash(content_hash),
                );

                let display_path = format_display_path(Path::new(&input.path), env.cwd.as_path());
                let xml = Output::new()
                    .element("file_removed")
                    .attr("path", display_path)
                    .attr("status", "completed")
                    .done()
                    .render_xml();
                
                // Generate markdown for ACP display
                let markdown = crate::operation_markdown::format_remove(
                    &input.path,
                    &output,
                    env.cwd.as_path(),
                );

                forge_domain::ToolOutput::paired(xml, markdown)
            }

            ToolOperation::FsSearch { input, output } => match output {
                Some(out) => {
                    let max_lines = min(
                        env.max_search_lines,
                        input.head_limit.unwrap_or(u32::MAX) as usize,
                    );
                    let offset = input.offset.unwrap_or(0) as usize;
                    let search_dir = Path::new(input.path.as_deref().unwrap_or("."));
                    let truncated_output = truncate_search_output(
                        &out.matches,
                        offset,
                        max_lines,
                        env.max_search_result_bytes,
                        search_dir,
                    );

                    let display_lines = if truncated_output.start < truncated_output.end {
                        // Use 1-based indexing for display (humans count from 1)
                        format!("{}-{}", truncated_output.start + 1, truncated_output.end)
                    } else {
                        // No matches or empty result
                        "0-0".to_string()
                    };

                    let mut builder = ElementBuilder::new("search_results")
                        .attr("path", input.path.as_deref().unwrap_or("."))
                        .attr("max_bytes_allowed", env.max_search_result_bytes.to_string())
                        .attr("total_lines", truncated_output.total.to_string())
                        .attr("display_lines", display_lines)
                        .attr("pattern", &input.pattern);

                    if let Some(glob) = input.glob.as_ref() {
                        builder = builder.attr("glob", glob);
                    }
                    if let Some(file_type) = input.file_type.as_ref() {
                        builder = builder.attr("file_type", file_type);
                    }

                    match truncated_output.strategy {
                        TruncationMode::Byte => {
                            let reason = format!(
                                "Results truncated due to exceeding the {} bytes size limit. Please use a more specific search pattern",
                                env.max_search_result_bytes
                            );
                            builder = builder.attr("reason", reason);
                        }
                        TruncationMode::Line => {
                            let reason = format!(
                                "Results truncated due to exceeding the {max_lines} lines limit. Please use a more specific search pattern"
                            );
                            builder = builder.attr("reason", reason);
                        }
                        TruncationMode::Full => {}
                    };
                    
                    let xml = Output::new()
                        .part(builder.cdata(truncated_output.data.join("\n")).build())
                        .render_xml();

                    // Generate markdown for ACP display
                    let markdown = crate::operation_markdown::format_search(
                        &input.pattern,
                        &out,
                        max_lines,
                    );

                    forge_domain::ToolOutput::paired(xml, markdown)
                }
                None => {
                    let mut builder = ElementBuilder::new("search_results");
                    
                    if let Some(path) = input.path {
                        builder = builder.attr("path", path);
                    }
                    builder = builder.attr("pattern", &input.pattern);
                    if let Some(glob) = input.glob {
                        builder = builder.attr("glob", glob);
                    }
                    if let Some(file_type) = input.file_type.as_ref() {
                        builder = builder.attr("file_type", file_type);
                    }
                    
                    let xml = Output::new().part(builder.build()).render_xml();
                    
                    // Generate markdown for ACP display
                    let markdown = crate::operation_markdown::format_search_empty(&input.pattern);

                    forge_domain::ToolOutput::paired(xml, markdown)
                }
            },
            ToolOperation::CodebaseSearch { output } => {
                let total_results: usize = output.queries.iter().map(|q| q.results.len()).sum();
                let mut root_builder = ElementBuilder::new("sem_search_results");

                if output.queries.is_empty() || total_results == 0 {
                    root_builder = root_builder.text("No results found for query. Try refining your search with more specific terms or different keywords.");
                } else {
                    for query_result in &output.queries {
                        let mut query_builder = ElementBuilder::new("query_result")
                            .attr("query", &query_result.query)
                            .attr("use_case", &query_result.use_case)
                            .attr("results", query_result.results.len().to_string());

                        let mut grouped_by_path: HashMap<&str, Vec<_>> = HashMap::new();

                        // Extract all file chunks and group by path
                        for data in &query_result.results {
                            if let forge_domain::NodeData::FileChunk(file_chunk) = &data.node {
                                let key = file_chunk.file_path.as_str();
                                grouped_by_path.entry(key).or_default().push(file_chunk);
                            }
                        }

                        // Sort by file path for stable ordering
                        let mut grouped_chunks: Vec<_> = grouped_by_path.into_iter().collect();
                        grouped_chunks.sort_by(|a, b| a.0.cmp(b.0));

                        // Process each file path
                        for (path, mut chunks) in grouped_chunks {
                            // Sort chunks by start line
                            chunks.sort_by_key(|a| a.start_line);

                            let mut content_parts = Vec::new();
                            for chunk in chunks {
                                let numbered =
                                    chunk.content.to_numbered_from(chunk.start_line as usize);
                                content_parts.push(numbered);
                            }

                            let data = content_parts.join("\n...\n");
                            query_builder = query_builder.child(
                                ElementBuilder::new("file")
                                    .attr("path", path)
                                    .cdata(data)
                                    .build()
                            );
                        }

                        root_builder = root_builder.child(query_builder.build());
                    }
                }

                let xml = Output::new().part(root_builder.build()).render_xml();
                
                // Generate markdown for ACP display
                let query = output.queries.first().map(|q| q.query.as_str()).unwrap_or("search");
                let markdown = crate::operation_markdown::format_codebase_search(query, total_results);

                forge_domain::ToolOutput::paired(xml, markdown)
            }
            ToolOperation::FsPatch { input, output } => {
                let diff_result = DiffFormat::format(&output.before, &output.after);
                let diff = console::strip_ansi_codes(diff_result.diff()).to_string();

                let mut builder = ElementBuilder::new("file_diff")
                    .attr("path", &input.file_path)
                    .attr("total_lines", output.after.lines().count().to_string())
                    .cdata(diff);

                if !output.errors.is_empty() {
                    builder = builder.child(create_validation_warning(&input.file_path, &output.errors));
                }

                *metrics = metrics.clone().insert(
                    input.file_path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_added(diff_result.lines_added())
                        .lines_removed(diff_result.lines_removed())
                        .content_hash(Some(output.content_hash.clone())),
                );

                // Create a FileDiff for IDE display (ACP)
                let file_diff = forge_domain::FileDiff {
                    path: input.file_path.clone(),
                    old_text: Some(output.before.clone()),
                    new_text: output.after.clone(),
                };

                // Return a Pair: XML text for LLM, FileDiff for IDE
                forge_domain::ToolOutput {
                    is_error: false,
                    values: vec![forge_domain::ToolValue::Pair(
                        Box::new(forge_domain::ToolValue::Text(Output::new().part(builder.build()).render_xml())),
                        Box::new(forge_domain::ToolValue::FileDiff(file_diff)),
                    )],
                }
            }
            ToolOperation::FsUndo { input, output } => {
                // Diff between snapshot state (after_undo) and modified state
                // (before_undo)
                let diff = DiffFormat::format(
                    output.after_undo.as_deref().unwrap_or(""),
                    output.before_undo.as_deref().unwrap_or(""),
                );
                let content_hash = output.after_undo.as_ref().map(|s| compute_hash(s));

                *metrics = metrics.clone().insert(
                    input.path.clone(),
                    FileOperation::new(tool_kind)
                        .lines_added(diff.lines_added())
                        .lines_removed(diff.lines_removed())
                        .content_hash(content_hash),
                );

                match (&output.before_undo, &output.after_undo) {
                    (None, None) => {
                        let xml = Output::new()
                            .element("file_undo")
                            .attr("path", &input.path)
                            .attr("status", "no_changes")
                            .done()
                            .render_xml();
                        let markdown = crate::operation_markdown::format_undo(&input.path, "unchanged (no changes to undo)");
                        forge_domain::ToolOutput::paired(xml, markdown)
                    }
                    (None, Some(after)) => {
                        let xml = Output::new()
                            .element("file_undo")
                            .attr("path", &input.path)
                            .attr("status", "created")
                            .attr("total_lines", after.lines().count().to_string())
                            .cdata(after)
                            .done()
                            .render_xml();
                        let markdown = crate::operation_markdown::format_undo(&input.path, "created (undid removal)");
                        forge_domain::ToolOutput::paired(xml, markdown)
                    }
                    (Some(before), None) => {
                        let xml = Output::new()
                            .element("file_undo")
                            .attr("path", &input.path)
                            .attr("status", "removed")
                            .attr("total_lines", before.lines().count().to_string())
                            .cdata(before)
                            .done()
                            .render_xml();
                        let markdown = crate::operation_markdown::format_undo(&input.path, "removed (undid creation)");
                        forge_domain::ToolOutput::paired(xml, markdown)
                    }
                    (Some(before), Some(after)) => {
                        // This diff is between modified state (before_undo) and snapshot
                        // state (after_undo)
                        let diff = DiffFormat::format(before, after);

                        let xml = Output::new()
                            .element("file_undo")
                            .attr("path", &input.path)
                            .attr("status", "restored")
                            .cdata(strip_ansi_codes(diff.diff()))
                            .done()
                            .render_xml();
                        let markdown = crate::operation_markdown::format_undo(&input.path, "restored to previous state");

                        forge_domain::ToolOutput::paired(xml, markdown)
                    }
                }
            }
            ToolOperation::NetFetch { input, output } => {
                let content_type = match output.context {
                    ResponseContext::Parsed => "text/markdown".to_string(),
                    ResponseContext::Raw => output.content_type,
                };
                let truncated_content =
                    truncate_fetch_content(&output.content, env.fetch_truncation_limit);
                
                let mut builder = ElementBuilder::new("http_response")
                    .attr("url", &input.url)
                    .attr("status_code", output.code.to_string())
                    .attr("start_char", "0")
                    .attr(
                        "end_char",
                        env.fetch_truncation_limit.min(output.content.len()).to_string(),
                    )
                    .attr("total_chars", output.content.len().to_string())
                    .attr("content_type", content_type)
                    .child(ElementBuilder::new("body").cdata(truncated_content.content).build());

                if let Some(path) = content_files.stdout {
                    builder = builder.child(
                        ElementBuilder::new("truncated")
                            .text(format!(
                                "Content is truncated to {} chars, remaining content can be read from path: {}",
                                env.fetch_truncation_limit, path.display()
                            ))
                            .build()
                    );
                }

                let elm = Output::new().part(builder.build()).render_xml();
                forge_domain::ToolOutput::text(elm)
            }
            ToolOperation::Shell { output } => {
                let mut parent_builder = ElementBuilder::new("shell_output")
                    .attr("command", &output.output.command)
                    .attr("shell", &output.shell);

                if let Some(description) = &output.description {
                    parent_builder = parent_builder.attr("description", description);
                }

                if let Some(exit_code) = output.output.exit_code {
                    parent_builder = parent_builder.attr("exit_code", exit_code.to_string());
                }

                let truncated_output = truncate_shell_output(
                    &output.output.stdout,
                    &output.output.stderr,
                    env.stdout_max_prefix_length,
                    env.stdout_max_suffix_length,
                    env.stdout_max_line_length,
                );

                if let Some(stdout_elem) = create_stream_element(
                    &truncated_output.stdout,
                    content_files.stdout.as_deref(),
                ) {
                    parent_builder = parent_builder.child(stdout_elem);
                }

                if let Some(stderr_elem) = create_stream_element(
                    &truncated_output.stderr,
                    content_files.stderr.as_deref(),
                ) {
                    parent_builder = parent_builder.child(stderr_elem);
                }

                let parent_elem = Output::new().part(parent_builder.build()).render_xml();
                forge_domain::ToolOutput::text(parent_elem)
            }
            ToolOperation::FollowUp { output } => match output {
                None => {
                    let elm = Output::new()
                        .element("interrupted")
                        .text("No feedback provided")
                        .done()
                        .render_xml();
                    forge_domain::ToolOutput::text(elm)
                }
                Some(content) => {
                    let elm = Output::new()
                        .element("feedback")
                        .text(content)
                        .done()
                        .render_xml();
                    forge_domain::ToolOutput::text(elm)
                }
            },
            ToolOperation::PlanCreate { input, output } => {
                let elm = Output::new()
                    .element("plan_created")
                    .attr("path", output.path.display().to_string())
                    .attr("plan_name", input.plan_name)
                    .attr("version", input.version.to_string())
                    .done()
                    .render_xml();

                forge_domain::ToolOutput::text(elm)
            }
            ToolOperation::Skill { output } => {
                let mut command_builder = ElementBuilder::new("command");
                if let Some(path) = output.path {
                    command_builder = command_builder.attr("location", path.display().to_string());
                }
                command_builder = command_builder.cdata(output.command);

                let mut skill_builder = ElementBuilder::new("skill_details")
                    .child(command_builder.build());

                // Insert Resources
                for resource in &output.resources {
                    skill_builder = skill_builder.child(
                        ElementBuilder::new("resource")
                            .text(resource.display().to_string())
                            .build()
                    );
                }

                let elm = Output::new().part(skill_builder.build()).render_xml();
                forge_domain::ToolOutput::text(elm)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;
    use std::path::PathBuf;

    use forge_domain::{FSRead, ToolValue};

    use super::*;
    use crate::{Content, Match, MatchResult};

    fn fixture_environment() -> Environment {
        use fake::{Fake, Faker};
        let max_bytes: f64 = 250.0 * 1024.0; // 250 KB
        let fixture: Environment = Faker.fake();
        fixture
            .cwd(PathBuf::from("/projects/test")) // Set deterministic cwd to avoid flaky path formatting
            .max_search_lines(25)
            .max_search_result_bytes(max_bytes.ceil() as usize)
            .fetch_truncation_limit(55)
            .max_read_size(10)
            .stdout_max_prefix_length(10)
            .stdout_max_suffix_length(10)
            .max_line_length(100)
            .max_file_size(256 << 10) // 256 KiB
    }

    fn to_value(output: forge_domain::ToolOutput) -> String {
        let values = output.values;
        let mut result = String::new();
        values.into_iter().for_each(|value| match value {
            ToolValue::Text(txt) => {
                writeln!(result, "{}", txt).unwrap();
            }
            ToolValue::Image(image) => {
                writeln!(result, "Image with mime type: {}", image.mime_type()).unwrap();
            }
            ToolValue::Empty => {
                writeln!(result, "Empty value").unwrap();
            }
            ToolValue::AI { value, .. } => {
                writeln!(result, "{}", value).unwrap();
            }
            ToolValue::FileDiff(file_diff) => {
                writeln!(
                    result,
                    "FileDiff: {} (old: {}, new: {})",
                    file_diff.path,
                    file_diff.old_text.as_ref().map_or(0, |t| t.len()),
                    file_diff.new_text.len()
                )
                .unwrap();
            }
            ToolValue::Markdown(md) => {
                writeln!(result, "Markdown: {}", md).unwrap();
            }
            ToolValue::Pair(llm, _display) => {
                // For tests, just show the LLM value
                match *llm {
                    ToolValue::Text(txt) => {
                        writeln!(result, "{}", txt).unwrap();
                    }
                    _ => {
                        writeln!(result, "Pair (LLM value not text)").unwrap();
                    }
                }
            }
        });

        result
    }

    /// Creates test syntax errors for testing purposes
    fn test_syntax_errors(errors: Vec<(u32, u32, &str)>) -> Vec<forge_domain::SyntaxError> {
        use forge_domain::SyntaxError;

        errors
            .into_iter()
            .map(|(line, column, message)| SyntaxError {
                line,
                column,
                message: message.to_string(),
            })
            .collect()
    }

    // Helper functions for semantic search tests
    mod sem_search_helpers {
        use fake::{Fake, Faker};
        use forge_domain::{CodebaseQueryResult, CodebaseSearchResults, FileChunk, Node, NodeData};

        /// Creates a file chunk node with auto-generated ID, computed end_line,
        /// and default relevance
        ///
        /// # Arguments
        /// * `file_path` - Path to the file
        /// * `content` - Code content
        /// * `start_line` - Starting line number
        ///
        /// The end_line is computed from content by counting newlines.
        /// Node ID is auto-generated using faker.
        /// Relevance defaults to 0.9.
        pub fn chunk_node(file_path: &str, content: &str, start_line: u32) -> Node {
            let line_count = content.lines().count() as u32;
            let end_line = start_line + line_count.saturating_sub(1);
            let relevance = 0.9;
            let node_id: String = Faker.fake();

            Node {
                node_id: node_id.into(),
                node: NodeData::FileChunk(FileChunk {
                    file_path: file_path.to_string(),
                    content: content.to_string(),
                    start_line,
                    end_line,
                }),
                relevance: Some(relevance),
                distance: Some(1.0 - relevance),
            }
        }

        /// Creates a CodebaseSearchResults with a single query
        pub fn search_results(
            query: &str,
            use_case: &str,
            nodes: Vec<Node>,
        ) -> CodebaseSearchResults {
            CodebaseSearchResults {
                queries: vec![CodebaseQueryResult {
                    query: query.to_string(),
                    use_case: use_case.to_string(),
                    results: nodes,
                }],
            }
        }
    }

    #[test]
    fn test_fs_read_basic() {
        let content = "Hello, world!\nThis is a test file.";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                file_path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 2,
                total_lines: 2,
                content_hash: hash,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Read,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_basic_special_chars() {
        let content = "struct Foo<T>{ name: T }";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                file_path: "/home/user/test.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 1,
                total_lines: 1,
                content_hash: hash,
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Read,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_explicit_range() {
        let content = "Line 1\nLine 2\nLine 3";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                file_path: "/home/user/test.txt".to_string(),
                start_line: Some(2),
                end_line: Some(3),
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 2,
                end_line: 3,
                total_lines: 5,
                content_hash: hash,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Read,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_with_truncation_path() {
        let content = "Truncated content";
        let hash = crate::compute_hash(content);
        let fixture = ToolOperation::FsRead {
            input: FSRead {
                file_path: "/home/user/large_file.txt".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::file(content),
                start_line: 1,
                end_line: 100,
                total_lines: 200,
                content_hash: hash,
            },
        };

        let env = fixture_environment();
        let truncation_path =
            TempContentFiles::default().stdout(PathBuf::from("/tmp/truncated_content.txt"));

        let actual = fixture.into_tool_output(
            ToolKind::Read,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_basic() {
        let content = "Hello, world!";
        let fixture = ToolOperation::FsWrite {
            input: forge_domain::FSWrite {
                file_path: "/home/user/new_file.txt".to_string(),
                content: content.to_string(),
                overwrite: false,
            },
            output: FsWriteOutput {
                path: "/home/user/new_file.txt".to_string(),
                before: None,
                errors: vec![],
                content_hash: compute_hash(content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_overwrite() {
        let content = "New content for the file";
        let fixture = ToolOperation::FsWrite {
            input: forge_domain::FSWrite {
                file_path: "/home/user/existing_file.txt".to_string(),
                content: content.to_string(),
                overwrite: true,
            },
            output: FsWriteOutput {
                path: "/home/user/existing_file.txt".to_string(),
                before: Some("Old content".to_string()),
                errors: vec![],
                content_hash: compute_hash(content),
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_no_truncation() {
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "echo hello".to_string(),
                    stdout: "hello\nworld".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_stdout_truncation_only() {
        // Create stdout with more lines than the truncation limit
        let mut stdout_lines = Vec::new();
        for i in 1..=25 {
            stdout_lines.push(format!("stdout line {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "long_command".to_string(),
                    stdout,
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let truncation_path =
            TempContentFiles::default().stdout(PathBuf::from("/tmp/stdout_content.txt"));
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_stderr_truncation_only() {
        // Create stderr with more lines than the truncation limit
        let mut stderr_lines = Vec::new();
        for i in 1..=25 {
            stderr_lines.push(format!("stderr line {}", i));
        }
        let stderr = stderr_lines.join("\n");

        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "error_command".to_string(),
                    stdout: "".to_string(),
                    stderr,
                    exit_code: Some(1),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let truncation_path =
            TempContentFiles::default().stderr(PathBuf::from("/tmp/stderr_content.txt"));
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_both_stdout_stderr_truncation() {
        // Create both stdout and stderr with more lines than the truncation limit
        let mut stdout_lines = Vec::new();
        for i in 1..=25 {
            stdout_lines.push(format!("stdout line {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let mut stderr_lines = Vec::new();
        for i in 1..=30 {
            stderr_lines.push(format!("stderr line {}", i));
        }
        let stderr = stderr_lines.join("\n");

        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "complex_command".to_string(),
                    stdout,
                    stderr,
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let truncation_path = TempContentFiles::default()
            .stdout(PathBuf::from("/tmp/stdout_content.txt"))
            .stderr(PathBuf::from("/tmp/stderr_content.txt"));
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_exact_boundary_stdout() {
        // Create stdout with exactly the truncation limit (prefix + suffix = 20 lines)
        let mut stdout_lines = Vec::new();
        for i in 1..=20 {
            stdout_lines.push(format!("stdout line {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "boundary_command".to_string(),
                    stdout,
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_single_line_each() {
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "simple_command".to_string(),
                    stdout: "single stdout line".to_string(),
                    stderr: "single stderr line".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_empty_streams() {
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "silent_command".to_string(),
                    stdout: "".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_output_line_number_calculation() {
        // Test specific line number calculations for 1-based indexing
        let mut stdout_lines = Vec::new();
        for i in 1..=15 {
            stdout_lines.push(format!("stdout {}", i));
        }
        let stdout = stdout_lines.join("\n");

        let mut stderr_lines = Vec::new();
        for i in 1..=12 {
            stderr_lines.push(format!("stderr {}", i));
        }
        let stderr = stderr_lines.join("\n");

        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "line_test_command".to_string(),
                    stdout,
                    stderr,
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();
        let truncation_path = TempContentFiles::default()
            .stdout(PathBuf::from("/tmp/stdout_content.txt"))
            .stderr(PathBuf::from("/tmp/stderr_content.txt"));
        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_output() {
        // Create a large number of search matches to trigger truncation
        let mut matches = Vec::new();
        let total_lines = 50;
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!("Match line {}: Test", i),
                    line_number: Some(i),
                }),
            });
        }

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "search".to_string(),
                glob: Some("*.txt".to_string()),
                ..Default::default()
            },
            output: Some(SearchResult { matches }),
        };

        let env = fixture_environment(); // max_search_lines is 25

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_max_output() {
        // Create a large number of search matches to trigger truncation
        let mut matches = Vec::new();
        let total_lines = 50; // Total lines found.
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!("Match line {}: Test", i),
                    line_number: Some(i),
                }),
            });
        }

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "search".to_string(),
                glob: Some("*.txt".to_string()),
                ..Default::default()
            },
            output: Some(SearchResult { matches }),
        };

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 10 for this test
        env.max_search_lines = 10;

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_min_lines_but_max_line_length() {
        // Create a large number of search matches to trigger truncation
        let mut matches = Vec::new();
        let total_lines = 50; // Total lines found.
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!("Match line {}: {}", i, "AB".repeat(50)),
                    line_number: Some(i),
                }),
            });
        }

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "search".to_string(),
                glob: Some("*.txt".to_string()),
                ..Default::default()
            },
            output: Some(SearchResult { matches }),
        };

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 20 for this test
        env.max_search_lines = 20;
        let max_bytes: f64 = 0.001 * 1024.0 * 1024.0;
        env.max_search_result_bytes = max_bytes.ceil() as usize; // limit to 0.001 MB

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_very_lengthy_one_line_match() {
        let mut matches = Vec::new();
        let total_lines = 1; // Total lines found.
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!(
                        "Match line {}: {}",
                        i,
                        "abcdefghijklmnopqrstuvwxyz".repeat(40)
                    ),
                    line_number: Some(i),
                }),
            });
        }

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "search".to_string(),
                glob: Some("*.txt".to_string()),
                ..Default::default()
            },
            output: Some(SearchResult { matches }),
        };

        let mut env = fixture_environment();
        // Total lines found are 50, but we limit to 20 for this test
        env.max_search_lines = 20;
        let max_bytes: f64 = 0.001 * 1024.0 * 1024.0;
        env.max_search_result_bytes = max_bytes.ceil() as usize; // limit to 0.001 MB

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_matches() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/empty_project".to_string()),
                pattern: "nonexistent".to_string(),
                ..Default::default()
            },
            output: None,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_with_warning() {
        let content = "Content with warning";
        let fixture = ToolOperation::FsWrite {
            input: forge_domain::FSWrite {
                file_path: "/home/user/file_with_warning.txt".to_string(),
                content: content.to_string(),
                overwrite: false,
            },
            output: FsWriteOutput {
                path: "/home/user/file_with_warning.txt".to_string(),
                before: None,
                errors: test_syntax_errors(vec![(10, 5, "Syntax error on line 10")]),
                content_hash: compute_hash(content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_create_with_warning_xml_tags() {
        let content = "Content with warning";
        let fixture = ToolOperation::FsWrite {
            input: forge_domain::FSWrite {
                file_path: "/home/user/file_with_warning.txt".to_string(),
                content: content.to_string(),
                overwrite: false,
            },
            output: FsWriteOutput {
                path: "/home/user/file_with_warning.txt".to_string(),
                before: None,
                errors: test_syntax_errors(vec![
                    (10, 5, "Syntax error on line 10"),
                    (20, 15, "Missing semicolon"),
                ]),
                content_hash: compute_hash(content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Write,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_remove_success() {
        let fixture = ToolOperation::FsRemove {
            input: forge_domain::FSRemove { path: "/home/user/file_to_delete.txt".to_string() },
            output: FsRemoveOutput { content: "content".to_string() },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Remove,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_results() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "Hello".to_string(),
                glob: Some("*.txt".to_string()),
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![
                    Match {
                        path: "file1.txt".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(1),
                            line: "Hello world".to_string(),
                        }),
                    },
                    Match {
                        path: "file2.txt".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(3),
                            line: "Hello universe".to_string(),
                        }),
                    },
                ],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_offset() {
        // Create 50 matches to test offset and pagination
        let mut matches = Vec::new();
        let total_lines = 50;
        for i in 1..=total_lines {
            matches.push(Match {
                path: "/home/user/project/foo.txt".to_string(),
                result: Some(MatchResult::Found {
                    line: format!("Match line {}: Test", i),
                    line_number: Some(i),
                }),
            });
        }

        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "search".to_string(),
                glob: Some("*.txt".to_string()),
                offset: Some(10),     // Skip first 10 matches
                head_limit: Some(15), // Take 15 matches after offset
                ..Default::default()
            },
            output: Some(SearchResult { matches }),
        };

        let env = fixture_environment(); // max_search_lines is 25

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_files_with_matches_mode() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "test".to_string(),
                output_mode: Some(forge_domain::OutputMode::FilesWithMatches),
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![
                    Match {
                        path: "/home/user/project/file1.rs".to_string(),
                        result: Some(MatchResult::FileMatch),
                    },
                    Match {
                        path: "/home/user/project/file2.rs".to_string(),
                        result: Some(MatchResult::FileMatch),
                    },
                ],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_count_mode() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "test".to_string(),
                output_mode: Some(forge_domain::OutputMode::Count),
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![
                    Match {
                        path: "/home/user/project/file1.rs".to_string(),
                        result: Some(MatchResult::Count { count: 5 }),
                    },
                    Match {
                        path: "/home/user/project/file2.rs".to_string(),
                        result: Some(MatchResult::Count { count: 3 }),
                    },
                    Match {
                        path: "/home/user/project/file3.rs".to_string(),
                        result: Some(MatchResult::Count { count: 12 }),
                    },
                ],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_context_lines() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "MATCH".to_string(),
                context: Some(2), // 2 lines before and after
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![Match {
                    path: "/home/user/project/test.txt".to_string(),
                    result: Some(MatchResult::ContextMatch {
                        line_number: Some(10),
                        line: "This is the MATCH line".to_string(),
                        before_context: vec![
                            "line 8 before context".to_string(),
                            "line 9 before context".to_string(),
                        ],
                        after_context: vec![
                            "line 11 after context".to_string(),
                            "line 12 after context".to_string(),
                        ],
                    }),
                }],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_before_context() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "ERROR".to_string(),
                before_context: Some(3), // 3 lines before
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![Match {
                    path: "/home/user/project/log.txt".to_string(),
                    result: Some(MatchResult::ContextMatch {
                        line_number: Some(50),
                        line: "ERROR: Something went wrong".to_string(),
                        before_context: vec![
                            "line 47: INFO startup".to_string(),
                            "line 48: DEBUG processing".to_string(),
                            "line 49: WARN slow operation".to_string(),
                        ],
                        after_context: vec![],
                    }),
                }],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_after_context() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "TODO".to_string(),
                after_context: Some(2), // 2 lines after
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![Match {
                    path: "/home/user/project/src/main.rs".to_string(),
                    result: Some(MatchResult::ContextMatch {
                        line_number: Some(15),
                        line: "// TODO: Implement this feature".to_string(),
                        before_context: vec![],
                        after_context: vec![
                            "fn main() {".to_string(),
                            "    println!(\"Hello\");".to_string(),
                        ],
                    }),
                }],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_without_line_numbers() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "function".to_string(),
                show_line_numbers: Some(false),
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![
                    Match {
                        path: "/home/user/project/app.js".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: None, // No line number when disabled
                            line: "function doSomething() {".to_string(),
                        }),
                    },
                    Match {
                        path: "/home/user/project/utils.js".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: None,
                            line: "function helper() {".to_string(),
                        }),
                    },
                ],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_with_file_type() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "class".to_string(),
                file_type: Some("py".to_string()), // Python files only
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![
                    Match {
                        path: "/home/user/project/models.py".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(1),
                            line: "class User:".to_string(),
                        }),
                    },
                    Match {
                        path: "/home/user/project/views.py".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(5),
                            line: "class HomeView:".to_string(),
                        }),
                    },
                ],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_case_insensitive() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "error".to_string(),
                case_insensitive: Some(true),
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![
                    Match {
                        path: "/home/user/project/log.txt".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(10),
                            line: "ERROR: Connection failed".to_string(),
                        }),
                    },
                    Match {
                        path: "/home/user/project/log.txt".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(15),
                            line: "error in processing".to_string(),
                        }),
                    },
                    Match {
                        path: "/home/user/project/log.txt".to_string(),
                        result: Some(MatchResult::Found {
                            line_number: Some(20),
                            line: "Error: Invalid input".to_string(),
                        }),
                    },
                ],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_multiline_pattern() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "struct.*\\{.*field".to_string(),
                multiline: Some(true),
                ..Default::default()
            },
            output: Some(SearchResult {
                matches: vec![Match {
                    path: "/home/user/project/types.rs".to_string(),
                    result: Some(MatchResult::Found {
                        line_number: Some(10),
                        line: "struct User {\n    field: String".to_string(),
                    }),
                }],
            }),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_search_no_results() {
        let fixture = ToolOperation::FsSearch {
            input: forge_domain::FSSearch {
                path: Some("/home/user/project".to_string()),
                pattern: "NonExistentPattern".to_string(),
                ..Default::default()
            },
            output: None,
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::FsSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_basic() {
        let after_content = "Hello universe\nThis is a test";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                file_path: "/home/user/test.txt".to_string(),
                old_string: "world".to_string(),
                new_string: "universe".to_string(),
                replace_all: false,
            },
            output: PatchOutput {
                errors: vec![],
                before: "Hello world\nThis is a test".to_string(),
                after: after_content.to_string(),
                content_hash: compute_hash(after_content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Patch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_with_warning() {
        let after_content = "line1\nnew line\nline2";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                file_path: "/home/user/large_file.txt".to_string(),
                old_string: "line1".to_string(),
                new_string: "\nnew line".to_string(),
                replace_all: false,
            },
            output: PatchOutput {
                errors: test_syntax_errors(vec![(5, 10, "Invalid syntax")]),
                before: "line1\nline2".to_string(),
                after: after_content.to_string(),
                content_hash: compute_hash(after_content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Patch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_patch_with_warning_special_chars() {
        let after_content = "line1\nnew line\nline2";
        let fixture = ToolOperation::FsPatch {
            input: forge_domain::FSPatch {
                file_path: "/home/user/test.zsh".to_string(),
                old_string: "line1".to_string(),
                new_string: "\nnew line".to_string(),
                replace_all: false,
            },
            output: PatchOutput {
                errors: test_syntax_errors(vec![
                    (
                        22,
                        1,
                        r#"Syntax error at 'function dim() { echo "${_DIM}${1}${RESET}"'"#,
                    ),
                    (25, 5, "Unexpected token"),
                    (30, 10, "Missing closing brace"),
                ]),
                before: "line1\nline2".to_string(),
                after: after_content.to_string(),
                content_hash: compute_hash(after_content),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Patch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_no_changes() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/unchanged_file.txt".to_string() },
            output: FsUndoOutput { before_undo: None, after_undo: None },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_created() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/new_file.txt".to_string() },
            output: FsUndoOutput {
                before_undo: None,
                after_undo: Some("New file content\nLine 2\nLine 3".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_removed() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/deleted_file.txt".to_string() },
            output: FsUndoOutput {
                before_undo: Some(
                    "Original file content\nThat was deleted\nDuring undo".to_string(),
                ),
                after_undo: None,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_file_restored() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/restored_file.txt".to_string() },
            output: FsUndoOutput {
                before_undo: Some("Original content\nBefore changes".to_string()),
                after_undo: Some("Modified content\nAfter restoration".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_undo_success() {
        let fixture = ToolOperation::FsUndo {
            input: forge_domain::FSUndo { path: "/home/user/test.txt".to_string() },
            output: FsUndoOutput {
                before_undo: Some("ABC".to_string()),
                after_undo: Some("PQR".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Undo,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_net_fetch_success() {
        let fixture = ToolOperation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com".to_string(),
                raw: Some(false),
            },
            output: HttpResponse {
                content: "# Example Website\n\nThis is some content from a website.".to_string(),
                code: 200,
                context: ResponseContext::Raw,
                content_type: "text/plain".to_string(),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Fetch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_net_fetch_truncated() {
        let env = fixture_environment();
        let truncated_content = "Truncated Content".to_string();
        let long_content = format!(
            "{}{}",
            "A".repeat(env.fetch_truncation_limit),
            &truncated_content
        );
        let fixture = ToolOperation::NetFetch {
            input: forge_domain::NetFetch {
                url: "https://example.com/large-page".to_string(),
                raw: Some(false),
            },
            output: HttpResponse {
                content: long_content,
                code: 200,
                context: ResponseContext::Parsed,
                content_type: "text/html".to_string(),
            },
        };

        let truncation_path =
            TempContentFiles::default().stdout(PathBuf::from("/tmp/forge_fetch_abc123.txt"));

        let actual = fixture.into_tool_output(
            ToolKind::Fetch,
            truncation_path,
            &env,
            &mut Metrics::default(),
        );

        // make sure that the content is truncated
        assert!(
            !actual
                .values
                .first()
                .unwrap()
                .as_str()
                .unwrap()
                .ends_with(&truncated_content)
        );
        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_success() {
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "ls -la".to_string(),
                    stdout: "total 8\ndrwxr-xr-x  2 user user 4096 Jan  1 12:00 .\ndrwxr-xr-x 10 user user 4096 Jan  1 12:00 ..".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: None,
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_shell_with_description() {
        let fixture = ToolOperation::Shell {
            output: ShellOutput {
                output: forge_domain::CommandOutput {
                    command: "git status".to_string(),
                    stdout: "On branch main\nnothing to commit, working tree clean".to_string(),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                },
                shell: "/bin/bash".to_string(),
                description: Some("Shows working tree status".to_string()),
            },
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Shell,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_with_question() {
        let fixture = ToolOperation::FollowUp {
            output: Some("Which file would you like to edit?".to_string()),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Followup,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_sem_search_with_results() {
        use sem_search_helpers::{chunk_node, search_results};

        let fixture = ToolOperation::CodebaseSearch {
            output: search_results(
                "retry mechanism with exponential backoff",
                "where is the retrying logic written",
                vec![
                    chunk_node(
                        "src/retry.rs",
                        "fn retry_with_backoff(max_attempts: u32) {\n    let mut delay = 100;\n    for attempt in 0..max_attempts {\n        if try_operation().is_ok() {\n            return;\n        }\n        thread::sleep(Duration::from_millis(delay));\n        delay *= 2;\n    }\n}",
                        10,
                    ),
                    chunk_node(
                        "src/http/client.rs",
                        "async fn request_with_retry(&self, url: &str) -> Result<Response> {\n    const MAX_RETRIES: usize = 3;\n    let mut backoff = ExponentialBackoff::default();\n    // Implementation...\n}",
                        45,
                    ),
                ],
            ),
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::SemSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_sem_search_with_usecase() {
        use sem_search_helpers::{chunk_node, search_results};

        let fixture = ToolOperation::CodebaseSearch {
            output: search_results(
                "authentication logic",
                "need to add similar auth to my endpoint",
                vec![chunk_node(
                    "src/auth.rs",
                    "fn authenticate_user(token: &str) -> Result<User> {\n    verify_jwt(token)\n}",
                    10,
                )],
            ),
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::SemSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_follow_up_no_question() {
        let fixture = ToolOperation::FollowUp { output: None };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Followup,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_sem_search_multiple_chunks_same_file_sorted() {
        use sem_search_helpers::{chunk_node, search_results};

        // Test that multiple chunks from the same file are sorted by start_line
        // Chunks are provided in non-sequential order: 100, 10, 50
        let fixture = ToolOperation::CodebaseSearch {
            output: search_results(
                "database operations",
                "finding all database query implementations",
                vec![
                    // Third chunk (lines 100-102) - provided first
                    chunk_node(
                        "src/database.rs",
                        "fn delete_user(id: u32) -> Result<()> {\n    db.execute(\"DELETE FROM users WHERE id = ?\", &[id])\n}",
                        100,
                    ),
                    // First chunk (lines 10-12) - provided second
                    chunk_node(
                        "src/database.rs",
                        "fn get_user(id: u32) -> Result<User> {\n    db.query(\"SELECT * FROM users WHERE id = ?\", &[id])\n}",
                        10,
                    ),
                    // Second chunk (lines 50-52) - provided third
                    chunk_node(
                        "src/database.rs",
                        "fn update_user(id: u32, name: &str) -> Result<()> {\n    db.execute(\"UPDATE users SET name = ? WHERE id = ?\", &[name, id])\n}",
                        50,
                    ),
                ],
            ),
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::SemSearch,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_skill_operation() {
        let fixture = ToolOperation::Skill {
            output: forge_domain::Skill::new(
                "test-skill",
                "This is a test skill command with instructions",
                "A test skill for demonstration",
            )
            .path("/home/user/.forge/skills/test-skill")
            .resources(vec![
                PathBuf::from("/home/user/.forge/skills/test-skill/resource1.txt"),
                PathBuf::from("/home/user/.forge/skills/test-skill/resource2.md"),
            ]),
        };

        let env = fixture_environment();

        let actual = fixture.into_tool_output(
            ToolKind::Skill,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        insta::assert_snapshot!(to_value(actual));
    }

    #[test]
    fn test_fs_read_image_with_vision_model() {
        use forge_domain::Image;

        let fixture = ToolOperation::FsRead {
            input: FSRead {
                file_path: "/home/user/test.png".to_string(),
                start_line: None,
                end_line: None,
                show_line_numbers: true,
            },
            output: ReadOutput {
                content: Content::image(Image::new_base64(
                    "base64_image_data".to_string(),
                    "image/png",
                )),
                start_line: 1,
                end_line: 1,
                total_lines: 1,
                content_hash: "hash123".to_string(),
            },
        };

        let env = fixture_environment();
        let actual = fixture.into_tool_output(
            ToolKind::Read,
            TempContentFiles::default(),
            &env,
            &mut Metrics::default(),
        );

        // Should return image content
        assert!(!actual.values.is_empty(), "Expected non-empty output");
        match &actual.values[0] {
            forge_domain::ToolValue::Image(_) => (), // Expected
            _ => panic!("Expected image output for vision model"),
        }
    }
}
