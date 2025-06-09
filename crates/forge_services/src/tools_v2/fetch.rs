use std::sync::Arc;

use anyhow::{anyhow, Context};
use forge_app::{FetchOutput, NetFetchService};
use forge_domain::ToolDescription;
use forge_tool_macros::ToolDescription;
use reqwest::{Client, Url};

use crate::{Clipper, FsWriteService, Infrastructure};

/// Fetch tool returns the content of MAX_LENGTH.
const MAX_LENGTH: usize = 40_000;

/// Retrieves content from URLs as markdown or raw text. Enables access to
/// current online information including websites, APIs and documentation. Use
/// for obtaining up-to-date information beyond training data, verifying facts,
/// or retrieving specific online content. Handles HTTP/HTTPS and converts HTML
/// to readable markdown by default. Cannot access private/restricted resources
/// requiring authentication. Respects robots.txt and may be blocked by
/// anti-scraping measures. For large pages, returns the first 40,000 characters
/// and stores the complete content in a temporary file for subsequent access.
#[derive(Debug, ToolDescription)]
pub struct ForgeFetch<F> {
    client: Client,
    infra: Arc<F>,
}

impl<F: Infrastructure> ForgeFetch<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { client: Client::new(), infra }
    }
}

impl<F: Infrastructure> ForgeFetch<F> {
    async fn check_robots_txt(&self, url: &Url) -> anyhow::Result<()> {
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

    async fn fetch_url(&self, url: &Url, force_raw: bool) -> anyhow::Result<(String, String, u16)> {
        self.check_robots_txt(url).await?;

        let response = self
            .client
            .get(url.as_str())
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch URL {}: {}", url, e))?;
        let code = response.status().as_u16();

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
            Ok((content, String::new(), code))
        } else {
            Ok((
                page_raw,
                format!(
                    "Content type {content_type} cannot be simplified to markdown; Raw content provided instead"),
                code
            ))
        }
    }
}

#[async_trait::async_trait]
impl<F: Infrastructure> NetFetchService for ForgeFetch<F> {
    async fn fetch(&self, url: String, raw: Option<bool>) -> anyhow::Result<FetchOutput> {
        let url = Url::parse(&url).with_context(|| format!("Failed to parse URL: {url}"))?;

        let (content, context, code) = self.fetch_url(&url, raw.unwrap_or(false)).await?;

        let original_length = content.len();
        let end = MAX_LENGTH.min(original_length);

        // Apply truncation directly
        let truncated = Clipper::from_start(MAX_LENGTH).clip(&content);
        // Create temp file only if content was truncated
        let temp_file_path = if truncated.is_truncated() {
            Some(
                self.infra
                    .file_write_service()
                    .write_temp("forge_fetch_", ".txt", &content)
                    .await?,
            )
        } else {
            None
        };

        // Determine output. If truncated then use truncated content else the actual.
        let output = truncated.prefix_content().unwrap_or_else(|| &content);

        Ok(FetchOutput {
            content: output.to_string(),
            code,
            url: url.to_string(),
            original_length,
            start_char: 0,
            end_char: end,
            context,
            max_length: MAX_LENGTH,
            path: temp_file_path,
            is_truncated: truncated.is_truncated(),
        })
    }
}
