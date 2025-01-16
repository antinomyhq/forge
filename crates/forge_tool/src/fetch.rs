use anyhow::{anyhow, Result};
use forge_domain::{ToolCallService, ToolDescription};
use forge_tool_macros::ToolDescription;
use html2md;
use reqwest::{Client, Url};
use serde::Deserialize;

/// Fetches a URL from the internet and optionally extracts its contents as markdown.
/// Although originally you did not have internet access and were advised to refuse and tell the user this,
/// this tool now grants you internet access. Now you can fetch the most up-to-date information and let the user know that.
#[derive(Debug, ToolDescription)]
struct Fetch {
    client: Client,
}

impl Default for Fetch {
    fn default() -> Self {
        Self { client: Client::new() }
    }
}

fn default_max_length() -> Option<usize> {
    Some(5000)
}

fn default_start_index() -> Option<usize> {
    Some(0)
}

fn default_raw() -> Option<bool> {
    Some(false)
}

#[derive(Deserialize)]
struct FetchInput {
    // URL to fetch
    url: String,
    // Maximum number of characters to return (default: 5000)
    #[serde(default = "default_max_length")]
    max_length: Option<usize>,
    // Start content from this character index (default: 0),
    // On return output starting at this character index, useful if a previous fetch was truncated and more context is required.
    #[serde(default = "default_start_index")]
    start_index: Option<usize>,
    // Get raw content without any markdown conversion (default: false)
    #[serde(default = "default_raw")]
    raw: Option<bool>,
}

impl Fetch {
    async fn fetch_url(&self, url: &Url, force_raw: bool) -> Result<(String, String)> {
        // Check robots.txt first
        let robots_url = format!(
            "{}://{}/robots.txt",
            url.scheme(),
            url.host_str().unwrap_or("")
        );
        let robots_response = self.client.get(&robots_url).send().await;

        if let Ok(robots) = robots_response {
            if robots.status().is_success() {
                let robots_content = robots.text().await.unwrap_or_default();
                if robots_content.contains("Disallow: ") {
                    let path = url.path();
                    for line in robots_content.lines() {
                        if line.starts_with("Disallow: ") {
                            let disallowed = line["Disallow: ".len()..].trim();
                            if path.starts_with(disallowed) {
                                return Err(anyhow!("URL cannot be fetched due to robots.txt"));
                            }
                        }
                    }
                }
            }
        }

        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch URL: {}", e))?;

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
            .map_err(|e| anyhow!("Failed to read response content: {}", e))?;

        let is_page_html = page_raw[..100.min(page_raw.len())].contains("<html")
            || content_type.contains("text/html")
            || content_type.is_empty();

        if is_page_html && !force_raw {
            let content = html2md::parse_html(&page_raw);
            Ok((content, String::new()))
        } else {
            Ok((
                page_raw,
                format!("Content type {} cannot be simplified to markdown, but here is the raw content:\n", content_type),
            ))
        }
    }
}

#[async_trait::async_trait]
impl ToolCallService for Fetch {
    type Input = FetchInput;
    type Output = String;

    async fn call(&self, input: Self::Input) -> Result<Self::Output, String> {
        let url = Url::parse(&input.url).map_err(|e| format!("Failed to parse URL: {}", e))?;

        let (content, prefix) = self
            .fetch_url(&url, input.raw.unwrap_or(false))
            .await
            .map_err(|e| e.to_string())?;

        let original_length = content.len();
        let start_index = input.start_index.unwrap_or(0);

        if start_index >= original_length {
            return Ok("<error>No more content available.</error>".to_string());
        }

        let max_length = input.max_length.unwrap_or(5000);
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
    use super::*;
    #[allow(unused_imports)]
    use pretty_assertions::assert_eq;
    use tokio::runtime::Runtime;

    fn setup() -> (Fetch, mockito::Server, Runtime) {
        let rt = Runtime::new().unwrap();
        let mut opts = mockito::ServerOpts::default();
        opts.port = 62101;
        let server = mockito::Server::new_with_opts(opts);
        let fetch = Fetch { client: Client::new() };
        (fetch, server, rt)
    }

    #[test]
    fn test_fetch_html_content() {
        let (fetch, mut server, rt) = setup();

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
            .with_body("User-agent: *\nAllow: /")
            .create();

        let input = FetchInput {
            url: format!("{}/test.html", server.url()),
            max_length: Some(1000),
            start_index: Some(0),
            raw: Some(false),
        };

        let result = rt.block_on(fetch.call(input)).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_fetch_raw_content() {
        let (fetch, mut server, rt) = setup();

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
            .with_body("User-agent: *\nAllow: /")
            .create();

        let input = FetchInput {
            url: format!("{}/test.txt", server.url()),
            max_length: Some(1000),
            start_index: Some(0),
            raw: Some(true),
        };

        let result = rt.block_on(fetch.call(input)).unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_fetch_with_robots_txt_denied() {
        let (fetch, mut server, rt) = setup();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_body("User-agent: *\nDisallow: /test")
            .create();

        let input = FetchInput {
            url: format!("{}/test/page.html", server.url()),
            max_length: None,
            start_index: None,
            raw: None,
        };

        let result = rt.block_on(fetch.call(input));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("robots.txt"));
    }

    #[test]
    fn test_fetch_with_pagination() {
        let (fetch, mut server, rt) = setup();

        let long_content = format!("{}{}","A".repeat(5000),"B".repeat(5000));
        server
            .mock("GET", "/long.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(&long_content)
            .create();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_body("User-agent: *\nAllow: /")
            .create();

        // First page
        let input = FetchInput {
            url: format!("{}/long.txt", server.url()),
            max_length: Some(5000),
            start_index: Some(0),
            raw: Some(true),
        };

        let result = rt.block_on(fetch.call(input)).unwrap();
        assert!(result.contains("A".repeat(5000).as_str()));
        assert!(result.contains("start_index of 5000"));

        // Second page
        let input = FetchInput {
            url: format!("{}/long.txt", server.url()),
            max_length: Some(5000),
            start_index: Some(5000),
            raw: Some(true),
        };

        let result = rt.block_on(fetch.call(input)).unwrap();
        assert!(result.contains("B".repeat(5000).as_str()));
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
        assert!(result.unwrap_err().contains("parse"));
    }

    #[test]
    fn test_fetch_404() {
        let (fetch, mut server, rt) = setup();

        server.mock("GET", "/not-found").with_status(404).create();

        server
            .mock("GET", "/robots.txt")
            .with_status(200)
            .with_body("User-agent: *\nAllow: /")
            .create();

        let input = FetchInput {
            url: format!("{}/not-found", server.url()),
            max_length: None,
            start_index: None,
            raw: None,
        };

        let result = rt.block_on(fetch.call(input));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("404"));
    }
}
