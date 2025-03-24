use anyhow::{anyhow, Context, Result};
use forge_display::TitleFormat;
use forge_domain::{ExecutableTool, NamedTool, ToolDescription};
use forge_tool_macros::ToolDescription;
use reqwest::{Client, Url};
use schemars::JsonSchema;
use serde::Deserialize;

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
        forge_domain::ToolName::new("tool_forge_net_fetch")
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
                            format!("/{}", disallowed)
                        } else {
                            disallowed.to_string()
                        };
                        let path = if !path.starts_with('/') {
                            format!("/{}", path)
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

    async fn fetch_url(&self, url: &Url, force_raw: bool) -> Result<(String, String)> {
        self.check_robots_txt(url).await?;

        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch URL {}: {}", url, e))?;

        println!(
            "{}",
            TitleFormat::execute(format!("GET {}", response.status()))
                .sub_title(url.as_str())
                .to_string()
                .as_str()
        );

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
                    "Content type {} cannot be simplified to markdown, but here is the raw content:\n",
                    content_type
                ),
            ))
        }
    }
}

#[async_trait::async_trait]
impl ExecutableTool for Fetch {
    type Input = FetchInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        let url = Url::parse(&input.url)
            .with_context(|| format!("Failed to parse URL: {}", input.url))?;

        let (content, prefix) = self.fetch_url(&url, input.raw.unwrap_or(false)).await?;

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
                "\n\n<error>Content truncated. Call the fetch tool with a start_index of {} to get more content.</error>",
                end
            ));
        }

        Ok(format!("{}Contents of {}:\n{}", prefix, url, truncated))
    }
}

#[cfg(test)]
mod tests {
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

        let result = fetch.call(input).await.unwrap();
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

        let result = fetch.call(input).await.unwrap();
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

        let result = fetch.call(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("robots.txt"),
            "Expected error containing 'robots.txt', got: {}",
            err
        );
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

        let result = fetch.call(input).await.unwrap();
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

        let result = fetch.call(input).await.unwrap();
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

        let result = rt.block_on(fetch.call(input));

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

        let result = fetch.call(input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("404"));
    }
}
