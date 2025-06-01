use std::sync::Arc;

use forge_display::TitleFormat;
use forge_domain::{
    CodebaseSearchInput, EnvironmentService, ExecutableTool, NamedTool, ToolCallContext,
    ToolDescription, ToolOutput,
};
use forge_tool_macros::ToolDescription;

use crate::{FsReadService, IndexerService, Infrastructure, QueryOptions};

/// Find snippets of code from the codebase most relevant to the search query.
/// This is a semantic search tool, so the query should ask for something
/// semantically matching what is needed. If it makes sense to only search in
/// particular directories, please specify them in the target_directories field.
/// Unless there is a clear reason to use your own search query, please just
/// reuse the user's exact query with their wording. Their exact
/// wording/phrasing can often be helpful for the semantic search query. Keeping
/// the same exact question format can also be helpful.
#[derive(ToolDescription)]
pub struct CodebaseSearch<F>(Arc<F>);

impl<F: Infrastructure> CodebaseSearch<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self(infra)
    }
}

impl<F: Infrastructure> NamedTool for CodebaseSearch<F> {
    fn tool_name() -> forge_domain::ToolName {
        forge_domain::ToolName::new("forge_tool_codebase_search")
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Offset {
    start: usize,
    end: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Span {
    start: Location,
    end: Location,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Location {
    line: usize,
    column: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct QueryOutput {
    path: String,
    kind: String,
    offset: Offset,
    span: Span,
}

#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for CodebaseSearch<F> {
    type Input = CodebaseSearchInput;

    async fn call(
        &self,
        context: ToolCallContext,
        input: Self::Input,
    ) -> anyhow::Result<ToolOutput> {
        context
            .send_text(TitleFormat::debug(
                match input.target_directories.as_ref() {
                    Some(dir) => {
                        format!(
                            "Codebase Search for '{}' in directories {:#?}",
                            input.query, dir
                        )
                    }
                    None => {
                        format!("Codebase Search for '{}'", input.query)
                    }
                },
            ))
            .await?;

        // Create query options and set paths if target directories are specified
        let mut options = QueryOptions::default();
        if let Some(dirs) = input.target_directories.as_ref() {
            options.path = Some(dirs.clone());
        }

        let results = self
            .0
            .indexer_service()
            .query::<QueryOutput>(&input.query, options)
            .await?;

        if results.is_empty() {
            return Ok(ToolOutput::text("No semantic searches found, try either changing the query or narrowing the search space".into()).is_error(true));
        };

        let mut code_snippets = String::new();

        let cwd = self.0.environment_service().get_environment().cwd;

        for result in results {
            let _ = context
                .send_text(TitleFormat::debug(format!(
                    "file: {}:{}:{}",
                    result.path, result.span.start.line, result.span.end.line
                )))
                .await;

            let file_path = cwd.join(&result.path);
            let code = match self.0.file_read_service().read_utf8(&file_path).await {
                Ok(file_content) => {
                    let content = &file_content[result.offset.start..=result.offset.end];
                    content.to_string()
                }
                Err(_e) => continue,
            };

            code_snippets.push_str(
                format!(
                    "<code_block path=\"{}\" char_start=\"{}\" char_end=\"{}\">\n",
                    result.path, result.offset.start, result.offset.end
                )
                .as_str(),
            );
            code_snippets.push_str(&code);
            code_snippets.push_str("\n</code_block>");
            code_snippets.push('\n');
        }

        let target_directories = match input.target_directories.as_ref() {
            Some(dirs) => format!(" target_directories=\"{}\"", dirs.join(",")),
            None => String::new(),
        };

        Ok(ToolOutput::text(format!(
            "<semantic_search query=\"{}\"{}>\n{}\n</semantic_search>",
            input.query, target_directories, code_snippets
        )))
    }
}
