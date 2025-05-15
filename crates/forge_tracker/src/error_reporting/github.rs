use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Local;
use inquire::Text;
use reqwest::Client;

use super::output;

// Constants for GitHub repository
pub const ORG: &str = "antinomyhq";
pub const REPO: &str = "forge";

/// GitHub issue creator that uses the GitHub API
pub struct GithubIssueCreator {
    client: Client,
    token: String,
}

impl GithubIssueCreator {
    pub fn new(token: String) -> Self {
        Self { client: Client::new(), token }
    }

    pub async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: Vec<String>,
    ) -> Result<String> {
        #[derive(serde::Serialize)]
        struct IssueRequest<'a> {
            title: &'a str,
            body: &'a str,
            labels: &'a [String],
        }

        let request = IssueRequest { title, body, labels: &labels };

        let response = self
            .client
            .post(format!("https://api.github.com/repos/{ORG}/{REPO}/issues",))
            .header("Accept", "application/vnd.github.v3+json")
            .header("Authorization", format!("token {}", self.token))
            .header("User-Agent", "forge-panic-reporter")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Failed to create GitHub issue: {}",
                error_text
            ));
        }

        #[derive(serde::Deserialize)]
        struct IssueResponse {
            html_url: String,
        }

        let issue: IssueResponse = response.json().await?;
        Ok(issue.html_url)
    }
}

/// Saves a crash report to a file and returns the path
pub fn save_crash_report_to_file(body: &str) -> Result<PathBuf> {
    // Get current working directory
    let cwd = std::env::current_dir()?;

    // Create a filename with date and timestamp
    let now = Local::now();
    let filename = format!(
        "crash-report-{}-{}.md",
        now.format("%m%d%y"),
        now.format("%H%M%S")
    );

    let filepath = cwd.join(&filename);

    // Write the report to the file
    fs::write(&filepath, body)?;

    output::success(format!("Crash report saved to: {}", filepath.display()));

    Ok(filepath)
}

/// Saves a GitHub token to the .env file
pub fn save_token_to_env(token: &str) -> Result<()> {
    // Get current working directory
    let cwd = std::env::current_dir()?;
    let env_path = cwd.join(".env");

    // Check if .env file exists
    let env_content = if env_path.exists() {
        // Read existing content
        let content = fs::read_to_string(&env_path)?;

        // Check if GITHUB_TOKEN is already defined
        if content
            .lines()
            .any(|line| line.starts_with("GITHUB_TOKEN="))
        {
            // Replace existing token
            let mut new_content = String::new();
            for line in content.lines() {
                if line.starts_with("GITHUB_TOKEN=") {
                    new_content.push_str(&format!("GITHUB_TOKEN={token}\n"));
                } else {
                    new_content.push_str(line);
                    new_content.push('\n');
                }
            }
            new_content
        } else {
            // Append token to file
            format!("{content}\nGITHUB_TOKEN={token}\n")
        }
    } else {
        // Create new file with token
        format!("GITHUB_TOKEN={token}\n")
    };

    // Write content to file
    fs::write(env_path, env_content)?;

    output::success("GitHub token saved to .env file in current directory");
    Ok(())
}

/// Prompts the user for a GitHub token
pub fn ask_for_github_token() -> Option<String> {
    match Text::new("Enter your GitHub token (leave empty to skip):").prompt() {
        Ok(token) if !token.trim().is_empty() => {
            // Save token to .env file if provided
            if let Err(e) = save_token_to_env(&token) {
                output::error_details("Failed to save token to .env file", e);
            }
            Some(token)
        }
        _ => None,
    }
}

/// Creates a GitHub issue via URL (browser redirect)
pub fn create_github_issue_via_url(title: &str, body: &str) -> Result<()> {
    // First save the report to a file due to URL length limitations
    let filepath = save_crash_report_to_file(body)?;

    // Use a minimal body that references the local file
    let minimal_body = format!(
        "# Crash Report\n\nDetailed crash report has been saved to: `{}`\n\nPlease attach this file when submitting the issue.",
        filepath.display()
    );

    // Encode title and minimal body for URL
    let encoded_title = url::form_urlencoded::byte_serialize(title.as_bytes()).collect::<String>();
    let encoded_body =
        url::form_urlencoded::byte_serialize(minimal_body.as_bytes()).collect::<String>();

    // Create the GitHub issue URL
    let issue_url = format!(
        "https://github.com/{ORG}/{REPO}/issues/new?title={encoded_title}&body={encoded_body}&labels=bug,crash"
    );

    output::action("Opening browser to create GitHub issue...");

    // Try to open the browser
    match open::that(&issue_url) {
        Ok(_) => output::success("Browser opened with GitHub issue form."),
        Err(e) => {
            output::error_details("Couldn't open browser automatically", e);
            output::instruction("Please visit this URL to create the issue:");
            output::raw(&issue_url);
        }
    }

    Ok(())
}
