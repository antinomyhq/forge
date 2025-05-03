use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, NamedTool, ToolCallContext, ToolDescription};
use forge_tool_macros::ToolDescription;
use reqwest::{Client, Url};
use schemars::JsonSchema;
use serde::Deserialize;
use tempfile::NamedTempFile;

/// Retrieves content from URLs as markdown or raw text. Enables access to
/// current online information including websites, APIs and documentation. Use
/// for obtaining up-to-date information beyond training data, verifying facts,
/// or retrieving specific online content. Handles HTTP/HTTPS and converts HTML
/// to readable markdown by default. Cannot access private/restricted resources
/// requiring authentication. Respects robots.txt and may be blocked by
/// anti-scraping measures. Large pages may require multiple requests with
/// adjusted start_index.
#[derive(Debug, ToolDescription)]
pub struct Fetch {
    client: Client,
}

impl NamedTool for Fetch {
    fn tool_name() -> forge_domain::ToolName {
        forge_domain::ToolName::new("forge_tool_net_fetch")
    }
}

impl Default for Fetch {
    fn default() -> Self {
        Self { client: Client::new() }
    }
}

fn default_start_index() -> Option<usize> {
    Some(0)
}

fn default_raw() -> Option<bool> {
    Some(false)
}

#[derive(Deserialize, JsonSchema)]
pub struct FetchInput {
    /// URL to fetch
    url: String,
    /// Maximum number of characters to return (default: 40000)
    max_length: Option<usize>,
    /// Start content from this character index (default: 0),
    /// On return output starting at this character index, useful if a previous
    /// fetch was truncated and more context is required.
    #[serde(default = "default_start_index")]
    start_index: Option<usize>,
    /// Get raw content without any markdown conversion (default: false)
    #[serde(default = "default_raw")]
    raw: Option<bool>,
}

impl Fetch {
    async fn check_robots_txt(&self, url: &Url) -> Result<()> {
        let robots_url = format!("{}://{}/robots.txt", url.scheme(), url.authority());
        let robots_response = self.client.get(&robots_url).send().await;

        if let Ok(robots) = robots_response {
            if robots.status().is_success() {
                let robots_content = robots.text().await.unwrap_or_default();
                let path = url.path();
                for line in robots_content.lines() {
                    if let Some(disallowed) = line.strip_prefix("Disallow: ") {
                        let disallowed = disallowed.trim();
                        let disallowed = if !disallowed.starts_with('/') {
                            format!("/{disallowed}")
                        } else {
                            disallowed.to_string()
                        };
                        let path = if !path.starts_with('/') {
                            format!("/{path}")
                        } else {
                            path.to_string()
                        };
                        if path.starts_with(&disallowed) {
                            return Err(anyhow!(
                                "URL {} cannot be fetched due to robots.txt restrictions",
                                url
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn fetch_url(
        &self,
        url: &Url,
        context: &ToolCallContext,
        force_raw: bool,
    ) -> Result<(String, String)> {
        self.check_robots_txt(url).await?;

        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch URL {}: {}", url, e))?;

        context
            .send_text(
                TitleFormat::debug(format!("GET {}", response.status())).sub_title(url.as_str()),
            )
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch {} - status code {}",
                url,
                response.status()
            ));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let page_raw = response
            .text()
            .await
            .map_err(|e| anyhow!("Failed to read response content from {}: {}", url, e))?;

        let is_page_html = page_raw[..100.min(page_raw.len())].contains("<html")
            || content_type.contains("text/html")
            || content_type.is_empty();

        if is_page_html && !force_raw {
            let content = html2md::parse_html(&page_raw);
            Ok((content, String::new()))
        } else {
            Ok((
                page_raw,
                format!(
                    "Content type {content_type} cannot be simplified to markdown, but here is the raw content:\n"),
            ))
        }
    }

    /// Writes content to a temporary file when it's too large to display
    fn save_to_temp_file(&self, content: &str) -> Result<PathBuf> {
        let mut temp_file = NamedTempFile::new()?;

        // Add a header comment with a TTL note
        let header = "# TEMPORARY FETCH RESULT\n";
        let ttl_comment = "# This temporary file may be deleted by the OS (e.g., on reboot).\n";
        let separator = format!("# {}\n\n", "-".repeat(70));

        // Write header and content
        temp_file.write_all(header.as_bytes())?;
        temp_file.write_all(ttl_comment.as_bytes())?;
        temp_file.write_all(separator.as_bytes())?;
        temp_file.write_all(content.as_bytes())?;

        // Keep the file and get its persistent path
        let persistent_path = temp_file.into_temp_path().keep()?;
        Ok(persistent_path)
    }
}

#[async_trait::async_trait]
impl ExecutableTool for Fetch {
    type Input = FetchInput;

    async fn call(&self, context: ToolCallContext, input: Self::Input) -> anyhow::Result<String> {
        let url = Url::parse(&input.url)
            .with_context(|| format!("Failed to parse URL: {}", input.url))?;

        let (content, prefix) = self
            .fetch_url(&url, &context, input.raw.unwrap_or(false))
            .await?;

        let original_length = content.len();
        let start_index = input.start_index.unwrap_or(0);

        if start_index >= original_length {
            return Ok("<error>No more content available.</error>".to_string());
        }

        let max_length = input.max_length.unwrap_or(40000);
        let end = (start_index + max_length).min(original_length);
        let mut truncated = content[start_index..end].to_string();

        if end < original_length {
            truncated.push_str(&format!(
                "\n\n<error>Content truncated. Call the fetch tool with a start_index of {end} to get more content.</error>"));
        }

        // If the content is very large (> 40k chars), store in a temp file
        if original_length > 40000 {
            let temp_file_path = self.save_to_temp_file(&content)?;

            // Create a response with temp file information
            return Ok(format!(
                "---\nURL: {}\ntotal_chars: {}\nstart_char: {}\nend_char: {}\ntemp_file: {}\n---\n{}",
                url,
                original_length,
                start_index,
                end,
                temp_file_path.display(),
                truncated
            ));
        }

        // Regular response for normal-sized content
        Ok(format!(
            "---\nURL: {}\ntotal_chars: {}\nstart_char: {}\nend_char: {}\n---\n{}",
            url, original_length, start_index, end, truncated
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use regex::Regex;
    use tokio::runtime::Runtime;

    use super::*;

    async fn setup() -> (Fetch, mockito::ServerGuard) {
        let server = mockito::Server::new_async().await;
        let fetch = Fetch { client: Client::new() };
        (fetch, server)
    }

    fn normalize_port(content: String) -> String {
        let re = Regex::new(r"http://127\.0\.0\.1:\d+").unwrap();
        re.replace_all(&content, "http://127.0.0.1:PORT")
            .to_string()
    }

    #[tokio::test]
    async fn test_fetch_html_content() {
        let (fetch, mut server) = setup().await;

        server
            .mock("GET", "/test.html")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body(
                r#"
                <html>
                    <body>
                        <h1>Test Title</h1>
                        <p>Test paragraph</p>
                    </body>
                </html>
            "#,
            )
            .create();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("User-agent: *\nAllow: /")
            .create();

        let input = FetchInput {
            url: format!("{}/test.html", server.url()),
            max_length: Some(1000),
            start_index: Some(0),
            raw: Some(false),
        };

        let result = fetch.call(ToolCallContext::default(), input).await.unwrap();
        let normalized_result = normalize_port(result);
        insta::assert_snapshot!(normalized_result);
    }

    #[tokio::test]
    async fn test_fetch_raw_content() {
        let (fetch, mut server) = setup().await;

        let raw_content = "This is raw text content";
        server
            .mock("GET", "/test.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(raw_content)
            .create();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("User-agent: *\nAllow: /")
            .create();

        let input = FetchInput {
            url: format!("{}/test.txt", server.url()),
            max_length: Some(1000),
            start_index: Some(0),
            raw: Some(true),
        };

        let result = fetch.call(ToolCallContext::default(), input).await.unwrap();
        let normalized_result = normalize_port(result);
        insta::assert_snapshot!(normalized_result);
    }

    #[tokio::test]
    async fn test_fetch_with_robots_txt_denied() {
        let (fetch, mut server) = setup().await;

        // Mock robots.txt request
        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("User-agent: *\nDisallow: /test")
            .create();

        // Mock the actual page request (though it shouldn't get this far)
        server
            .mock("GET", "/test/page.html")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body("<html><body>Test page</body></html>")
            .create();

        let input = FetchInput {
            url: format!("{}/test/page.html", server.url()),
            max_length: None,
            start_index: None,
            raw: None,
        };

        let result = fetch.call(ToolCallContext::default(), input).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("robots.txt"),
            "Expected error containing 'robots.txt', got: {err}"
        );
    }

    #[tokio::test]
    async fn test_fetch_large_content_creates_temp_file() {
        let (fetch, mut server) = setup().await;

        // Create content larger than 40k chars
        let large_content = "A".repeat(45000);

        server
            .mock("GET", "/large.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(&large_content)
            .create();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("User-agent: *\nAllow: /")
            .create();

        let input = FetchInput {
            url: format!("{}/large.txt", server.url()),
            max_length: Some(40000),
            start_index: Some(0),
            raw: Some(true),
        };

        let result = fetch.call(ToolCallContext::default(), input).await.unwrap();

        // Verify the response contains temp_file field
        assert!(result.contains("temp_file:"));

        // Extract the temp file path and verify it exists
        let re = Regex::new(r"temp_file: (.*)\n").unwrap();
        if let Some(captures) = re.captures(&result) {
            if let Some(path_match) = captures.get(1) {
                let temp_path = Path::new(path_match.as_str());
                assert!(temp_path.exists(), "Temp file should exist");

                // Verify the temp file contains the full content
                let file_content = fs::read_to_string(temp_path).unwrap();

                // Check that the file contains the header comments
                assert!(file_content.contains("TEMPORARY FETCH RESULT"));
                assert!(file_content.contains("may be deleted by the OS"));

                // Check that the file contains all the original content
                assert!(file_content.contains(&large_content));

                // Cleanup - delete the temp file
                fs::remove_file(temp_path).unwrap_or_default();
            }
        } else {
            panic!("Temp file path not found in response");
        }
    }

    #[tokio::test]
    async fn test_fetch_with_pagination() {
        let (fetch, mut server) = setup().await;

        let long_content = format!("{}{}", "A".repeat(5000), "B".repeat(5000));
        server
            .mock("GET", "/long.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(&long_content)
            .create();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("User-agent: *\nAllow: /")
            .create();

        // First page
        let input = FetchInput {
            url: format!("{}/long.txt", server.url()),
            max_length: Some(5000),
            start_index: Some(0),
            raw: Some(true),
        };

        let result = fetch.call(ToolCallContext::default(), input).await.unwrap();
        let normalized_result = normalize_port(result);
        assert!(normalized_result.contains("A".repeat(5000).as_str()));
        assert!(normalized_result.contains("start_index of 5000"));

        // Second page
        let input = FetchInput {
            url: format!("{}/long.txt", server.url()),
            max_length: Some(5000),
            start_index: Some(5000),
            raw: Some(true),
        };

        let result = fetch.call(ToolCallContext::default(), input).await.unwrap();
        let normalized_result = normalize_port(result);
        assert!(normalized_result.contains("B".repeat(5000).as_str()));
    }

    #[test]
    fn test_fetch_invalid_url() {
        let fetch = Fetch::default();
        let rt = Runtime::new().unwrap();

        let input = FetchInput {
            url: "not a valid url".to_string(),
            max_length: None,
            start_index: None,
            raw: None,
        };

        let result = rt.block_on(fetch.call(ToolCallContext::default(), input));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("parse"));
    }

    #[tokio::test]
    async fn test_fetch_404() {
        let (fetch, mut server) = setup().await;

        server.mock("GET", "/not-found").with_status(404).create();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("User-agent: *\nAllow: /")
            .create();

        let input = FetchInput {
            url: format!("{}/not-found", server.url()),
            max_length: None,
            start_index: None,
            raw: None,
        };

        let result = fetch.call(ToolCallContext::default(), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("404"));
    }
}
