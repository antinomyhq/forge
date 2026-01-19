use std::path::PathBuf;

use forge_domain::Attachment;

use crate::services::FsReadService;

/// Augments codebase_search agent output by reading referenced files
/// and including code snippets inline.
pub struct CodebaseSearchAugmenter<'a, F> {
    fs_read_service: &'a F,
    cwd: PathBuf,
}

impl<'a, F: FsReadService> CodebaseSearchAugmenter<'a, F> {
    pub fn new(fs_read_service: &'a F, cwd: PathBuf) -> Self {
        Self { fs_read_service, cwd }
    }

    /// Augments the output by parsing @[path:start:end] tags and
    /// including code snippets for each referenced location.
    pub async fn augment(&self, output: String) -> String {
        let lines = output.lines();
        let mut result = Vec::with_capacity(lines.count());
        for line in output.lines() {
            let line_tags = Attachment::parse_all(line);
            if line_tags.is_empty() {
                result.push(line.to_string());
                continue;
            }

            for tag in line_tags {
                result.push(line.to_string());
                if let Some(snippet) = self.read_file_snippet(&tag).await {
                    result.push(snippet);
                }
                result.push(String::new());
            }
        }

        result.join("\n")
    }

    /// Reads a file snippet for a given tag and returns it as a formatted code
    /// block.
    async fn read_file_snippet(&self, tag: &forge_domain::FileTag) -> Option<String> {
        let file_path = PathBuf::from(&tag.path);
        let (start_line, end_line) = match &tag.loc {
            Some(loc) => (loc.start, loc.end),
            None => (None, None),
        };

        // Normalize the path by joining with cwd if needed, then canonicalize to get
        // absolute path
        let normalized_path = if file_path.is_absolute() {
            file_path
        } else {
            self.cwd.join(&file_path)
        };

        let absolute_path = normalized_path.canonicalize().ok()?;

        let read_result = self
            .fs_read_service
            .read(
                absolute_path.to_string_lossy().to_string(),
                start_line,
                end_line,
            )
            .await;

        let read_output = read_result.ok()?;

        if read_output.content.as_image().is_some() {
            return None;
        }

        let lang = tag.path.split('.').next_back().unwrap_or("text");
        let content = read_output.content.file_content();
        Some(format!("```{lang}\n{content}\n```"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::services::{Content, ReadOutput};

    /// Mock FsReadService for testing
    struct MockFsReadService {
        files: HashMap<String, String>,
    }

    impl MockFsReadService {
        fn new() -> Self {
            let mut files = HashMap::new();
            files.insert(
                "test.rs".to_string(),
                "fn test_function() {\n    println!(\"hello\");\n}".to_string(),
            );
            files.insert(
                "main.py".to_string(),
                "def main():\n    print('hello')\n".to_string(),
            );
            files.insert(
                "config.json".to_string(),
                "{\n  \"key\": \"value\"\n}".to_string(),
            );
            Self { files }
        }
    }

    #[async_trait::async_trait]
    impl FsReadService for MockFsReadService {
        async fn read(
            &self,
            path: String,
            start_line: Option<u64>,
            end_line: Option<u64>,
        ) -> anyhow::Result<ReadOutput> {
            // Look up content in the HashMap
            let content = self
                .files
                .iter()
                .find(|(key, _): &(&String, &String)| path.contains(key.as_str()))
                .map(|(_, value)| value.as_str())
                .unwrap_or("unknown file");

            Ok(ReadOutput {
                content: Content::file(content.to_string()),
                start_line: start_line.unwrap_or(1),
                end_line: end_line.unwrap_or(content.lines().count() as u64),
                total_lines: content.lines().count() as u64,
                content_hash: "test_hash".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_augment_with_no_tags() {
        let mock_service = MockFsReadService::new();
        let cwd = PathBuf::from("/workspace");
        let augmenter = CodebaseSearchAugmenter::new(&mock_service, cwd);

        let output = "This is plain text\nwith no tags\nat all".to_string();
        let result = augmenter.augment(output.clone()).await;

        assert_eq!(result, output);
    }

    #[tokio::test]
    async fn test_augment_with_single_tag() {
        let mock_service = MockFsReadService::new();
        let cwd = PathBuf::from("/workspace");
        let augmenter = CodebaseSearchAugmenter::new(&mock_service, cwd);

        let output = "@[/workspace/test.rs:1:3] - Test function".to_string();
        let result = augmenter.augment(output).await;

        let expected = "@[/workspace/test.rs:1:3] - Test function\n```rs\nfn test_function() {\n    println!(\"hello\");\n}\n```\n";
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_augment_with_single_tag_() {
        let mock_service = MockFsReadService::new();
        let cwd = PathBuf::from("/workspace");
        let augmenter = CodebaseSearchAugmenter::new(&mock_service, cwd);

        let output = "@[/workspace/test.rs:1-3] - Test function".to_string();
        let result = augmenter.augment(output).await;

        let expected = "@[/workspace/test.rs:1-3] - Test function\n```rs\nfn test_function() {\n    println!(\"hello\");\n}\n```\n";
        assert_eq!(result, expected);
    }
}
