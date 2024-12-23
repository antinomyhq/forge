use forge_tool_macros::Description as DescriptionDerive;
use nom::bytes::complete::{tag, take_until};
use nom::character::complete::multispace0;
use nom::multi::many0;
use nom::{IResult, Parser};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Description, ToolTrait};

#[derive(Deserialize, JsonSchema)]
pub struct FSReplaceInput {
    pub path: String,
    pub diff: String,
}

/// Replace content in a file using SEARCH/REPLACE blocks. Each block defines
/// exact changes to make to specific parts of the file. Supports multiple
/// blocks for complex changes while preserving file formatting and structure.
#[derive(DescriptionDerive)]
pub struct FSReplace;

fn parse_block(input: &str) -> IResult<&str, (&str, &str)> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("<<<<<<< SEARCH")(input)?;
    let (input, search) = take_until("=======")(input)?;
    let (input, _) = tag("=======")(input)?;
    let (input, replace) = take_until(">>>>>>> REPLACE")(input)?;
    let (input, _) = tag(">>>>>>> REPLACE")(input)?;
    let search = search.trim();
    let replace = replace.trim();
    Ok((input, (search, replace)))
}

fn parse_diff(input: &str) -> IResult<&str, Vec<(&str, &str)>> {
    many0(parse_block).parse(input)
}
#[async_trait::async_trait]
impl ToolTrait for FSReplace {
    type Input = FSReplaceInput;
    type Output = String;

    async fn call(&self, input: Self::Input) -> Result<Self::Output, String> {
        let content = tokio::fs::read_to_string(&input.path)
            .await
            .map_err(|e| e.to_string())?;

        let (_, blocks) = parse_diff(&input.diff).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        let mut lines = content.lines().collect::<Vec<_>>();

        for (search, replace) in blocks {
            let search_lines = search.lines().collect::<Vec<_>>();
            let replace_lines = replace.lines().collect::<Vec<_>>();

            let mut i = 0;

            while i < lines.len() {
                // Check if the current segment matches the search block
                if i + search_lines.len() <= lines.len()
                    && lines[i..i + search_lines.len()] == *search_lines.as_slice()
                {
                    // Replace the matched lines
                    if !replace_lines.is_empty() {
                        result.extend_from_slice(&replace_lines);
                    }
                    i += search_lines.len(); // Skip the matched lines
                } else {
                    result.push(lines[i]);
                    i += 1;
                }
            }

            // Prepare lines for next block
            lines = result.clone();
            result.clear();
        }

        let new_content = lines.join("\n");

        tokio::fs::write(&input.path, &new_content)
            .await
            .map_err(|e| e.to_string())?;

        Ok(format!("Successfully replaced content in {}", input.path))
    }
}

#[cfg(test)]
mod test {
    use tempfile::TempDir;
    use tokio::fs;

    use super::*;

    #[tokio::test]
    async fn test_fs_replace_single_block() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello World\nTest Line\nGoodbye World";

        fs::write(&file_path, content).await.unwrap();

        let fs_replace = FSReplace;
        let result = fs_replace
            .call(FSReplaceInput {
                path: file_path.to_string_lossy().to_string(),
                diff: "<<<<<<< SEARCH\nHello World\n=======\nHi World\n>>>>>>> REPLACE".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully replaced"));

        let result = fs_replace
            .call(FSReplaceInput {
                path: file_path.to_string_lossy().to_string(),
                diff: "<<<<<<< SEARCH\nGoodbye=======\nBye\n>>>>>>> REPLACE".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully replaced"));

        let new_content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(new_content, "Hi World\nTest Line\nGoodbye World");
    }

    #[tokio::test]
    async fn test_fs_replace_multiple_blocks() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello World\nTest Line\nGoodbye World";

        fs::write(&file_path, content).await.unwrap();

        let fs_replace = FSReplace;
        let diff = "<<<<<<< SEARCH\nHello World\n=======\nHi World\n>>>>>>> REPLACE\n\n<<<<<<< SEARCH\nGoodbye World\n=======\nBye World\n>>>>>>> REPLACE".to_string();

        let result = fs_replace
            .call(FSReplaceInput { path: file_path.to_string_lossy().to_string(), diff })
            .await
            .unwrap();

        assert!(result.contains("Successfully replaced"));

        let new_content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(new_content, "Hi World\nTest Line\nBye World");
    }

    #[tokio::test]
    async fn test_fs_replace_no_match() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello World\nTest Line\nGoodbye World";

        fs::write(&file_path, content).await.unwrap();

        let fs_replace = FSReplace;
        let result = fs_replace
            .call(FSReplaceInput {
                path: file_path.to_string_lossy().to_string(),
                diff: "<<<<<<< SEARCH\nNo Match\n=======\nReplacement\n>>>>>>> REPLACE".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully replaced"));

        let new_content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(new_content, content);
    }

    #[tokio::test]
    async fn test_fs_replace_empty_block() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "Hello World\nTest Line\nGoodbye World";

        fs::write(&file_path, content).await.unwrap();

        let fs_replace = FSReplace;
        let result = fs_replace
            .call(FSReplaceInput {
                path: file_path.to_string_lossy().to_string(),
                diff: "<<<<<<< SEARCH\nTest Line\n=======\n>>>>>>> REPLACE".to_string(),
            })
            .await
            .unwrap();

        assert!(result.contains("Successfully replaced"));

        let new_content = fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(new_content, "Hello World\nGoodbye World");
    }
}
