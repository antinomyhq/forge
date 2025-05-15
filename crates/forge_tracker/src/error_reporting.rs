use anyhow::Result;
use chrono::Local;
use inquire::{Confirm, Text};
use reqwest::Client;
use std::fs;
use std::panic::{self, PanicHookInfo};
use std::path::PathBuf;
use std::{backtrace::Backtrace, fmt::Write as _};
use sysinfo::System;

use crate::{EventKind, Tracker};

const ORG: &str = "antinomyhq";
const REPO: &str = "forge";

#[derive(Debug, Clone, serde::Serialize)]
pub struct PanicReport {
    pub message: String,
    pub stack_trace: String,
    pub system_info: SystemInfo,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemInfo {
    pub os_name: String,
    pub cpu_cores: usize,
    pub memory_total: u64,
    pub app_version: String,
}

impl PanicReport {
    pub fn new(message: String, stack_trace: String) -> Self {
        Self { message, stack_trace, system_info: SystemInfo::collect() }
    }

    pub fn from_panic_info(info: &PanicHookInfo) -> Self {
        let backtrace = Backtrace::force_capture();
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = if let Some(location) = info.location() {
            format!(" at {}:{}", location.file(), location.line())
        } else {
            "".to_string()
        };

        Self::new(
            format!("{}{}", message, location),
            format!("{:#?}", backtrace),
        )
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        writeln!(&mut md, "# Crash Report").ok();
        writeln!(&mut md, "\n## Error\n").ok();
        writeln!(&mut md, "```").ok();
        writeln!(&mut md, "{}", self.message).ok();
        writeln!(&mut md, "```").ok();
        writeln!(&mut md, "\n## Stack Trace\n").ok();
        writeln!(&mut md, "```").ok();
        writeln!(&mut md, "{}", self.stack_trace).ok();
        writeln!(&mut md, "```").ok();
        writeln!(&mut md, "\n## System Information\n").ok();
        writeln!(&mut md, "- OS: {}", self.system_info.os_name).ok();
        writeln!(&mut md, "- CPU Cores: {}", self.system_info.cpu_cores).ok();
        writeln!(
            &mut md,
            "- Memory: {} MB",
            self.system_info.memory_total / 1024 / 1024
        )
        .ok();
        writeln!(&mut md, "- App Version: {}", self.system_info.app_version).ok();
        md
    }
}

impl SystemInfo {
    pub fn collect() -> Self {
        let sys = System::new_all();
        let version = match option_env!("APP_VERSION") {
            Some(val) => val.to_string(),
            None => env!("CARGO_PKG_VERSION").to_string(),
        };

        Self {
            os_name: System::long_os_version().unwrap_or_else(|| "Unknown".to_string()),
            cpu_cores: sys.physical_core_count().unwrap_or(0),
            memory_total: sys.total_memory(),
            app_version: version,
        }
    }
}

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

fn save_crash_report_to_file(body: &str) -> Result<PathBuf> {
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

    println!("Crash report saved to: {}", filepath.display());

    Ok(filepath)
}

fn save_token_to_env(token: &str) -> Result<()> {
    // Get current working directory
    let cwd = std::env::current_dir()?;
    let env_path = cwd.join(".env");
    
    // Check if .env file exists
    let env_content = if env_path.exists() {
        // Read existing content
        let content = fs::read_to_string(&env_path)?;
        
        // Check if GITHUB_TOKEN is already defined
        if content.lines().any(|line| line.starts_with("GITHUB_TOKEN=")) {
            // Replace existing token
            let mut new_content = String::new();
            for line in content.lines() {
                if line.starts_with("GITHUB_TOKEN=") {
                    new_content.push_str(&format!("GITHUB_TOKEN={}\n", token));
                } else {
                    new_content.push_str(line);
                    new_content.push('\n');
                }
            }
            new_content
        } else {
            // Append token to file
            format!("{}\nGITHUB_TOKEN={}\n", content, token)
        }
    } else {
        // Create new file with token
        format!("GITHUB_TOKEN={}\n", token)
    };
    
    // Write content to file
    fs::write(env_path, env_content)?;
    
    println!("GitHub token saved to .env file in current directory");
    Ok(())
}

fn ask_for_github_token() -> Option<String> {
    match Text::new("Enter your GitHub token (leave empty to skip):").prompt() {
        Ok(token) if !token.trim().is_empty() => {
            // Save token to .env file if provided
            if let Err(e) = save_token_to_env(&token) {
                eprintln!("Failed to save token to .env file: {}", e);
            }
            Some(token)
        },
        _ => None,
    }
}

fn create_github_issue_via_url(title: &str, body: &str) -> Result<()> {
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
        "https://github.com/{ORG}/{REPO}/issues/new?title={}&body={}&labels=bug,crash",
        encoded_title, encoded_body
    );

    println!("Opening browser to create GitHub issue...");

    // Try to open the browser
    match open::that(&issue_url) {
        Ok(_) => println!("Browser opened with GitHub issue form."),
        Err(e) => {
            println!("Couldn't open browser automatically: {}", e);
            println!("Please visit this URL to create the issue:");
            println!("{}", issue_url);
        }
    }

    Ok(())
}

pub fn install_panic_hook() {
    dotenv::dotenv().ok();
    panic::set_hook(Box::new(move |panic_info| {
        let report = PanicReport::from_panic_info(panic_info);
        let report_clone = report.clone();
        let rt_ph = tokio::runtime::Runtime::new().unwrap();
        let rt_gh = tokio::runtime::Runtime::new().unwrap();

        // Send report to PostHog
        let _ = std::thread::spawn(move || {
            rt_ph.block_on(async {
                // collect the event
                let _ = Tracker::default()
                    .dispatch(EventKind::Panic(
                        serde_json::to_string(&report_clone).unwrap(),
                    ))
                    .await;
            });
        })
        .join();

        // Ask user if they want to create a GitHub issue
        let report_formatted = report.to_markdown();
        // Print crash information
        eprintln!("\n\n{}\n\n", report.message);
        let _ = std::thread::spawn(move || {

            // Ask user if they want to create a GitHub issue
            match Confirm::new("Would you like to create a GitHub issue with the crash report?")
                .with_default(true)
                .prompt()
            {
                Ok(true) => {
                    // Save the crash report to file first to ensure it's always saved
                    match save_crash_report_to_file(&report_formatted) {
                        Ok(filepath) => {
                            println!("Crash report saved to: {}", filepath.display());
                        }
                        Err(e) => {
                            eprintln!("Failed to save crash report: {}", e);
                        }
                    }

                    // Generate a title for the issue
                    let title = "Crash Report".to_string();

                    // First try to get token from environment
                    let mut github_token = std::env::var("GITHUB_TOKEN").ok();

                    // If token exists in environment, make sure it's also saved to .env
                    if let Some(ref token) = github_token {
                        if !token.trim().is_empty() {
                            if let Err(e) = save_token_to_env(token) {
                                eprintln!("Failed to save token from environment to .env file: {}", e);
                            }
                        }
                    } else {
                        // If no token in environment, ask for one (which will also save it)
                        println!("No GitHub token found in environment.");
                        github_token = ask_for_github_token();
                    }

                    if let Some(token) = github_token {
                        if token.trim().is_empty() {
                            // Empty token provided, redirect to GitHub issues page
                            println!("No token provided.");
                            if let Err(e) = create_github_issue_via_url(&title, &report_formatted) {
                                eprintln!("Failed to open GitHub issue URL: {}", e);
                                println!("Please create an issue manually at: https://github.com/{ORG}/{REPO}/issues/new");
                            }
                        } else {
                            // Use the GitHub API with the provided token
                            println!("Creating GitHub issue using API...");
                            match rt_gh.block_on(async {
                                let creator = GithubIssueCreator::new(token);
                                creator
                                    .create_issue(
                                        &title,
                                        &report_formatted,
                                        vec!["bug".to_string(), "crash".to_string()],
                                    )
                                    .await
                            }) {
                                Ok(issue_url) => {
                                    println!("GitHub issue created successfully: {}", issue_url);
                                    // Try to open the browser
                                    if let Err(e) = open::that(&issue_url) {
                                        println!("Couldn't open browser automatically: {}", e);
                                        println!("Please visit the issue URL manually: {}", issue_url);
                                    }
                                }
                                Err(e) => {
                                    println!("Failed to create GitHub issue via API: {}", e);
                                    // Fallback to URL method
                                    if let Err(url_err) = create_github_issue_via_url(&title, &report_formatted) {
                                        eprintln!("Also failed to open GitHub issue URL: {}", url_err);
                                        println!("Please create an issue manually at: https://github.com/{ORG}/{REPO}/issues/new");
                                    }
                                }
                            }
                        }
                    } else {
                        // No token was provided, redirect to GitHub issues page
                        if let Err(e) = create_github_issue_via_url(&title, &report_formatted) {
                            eprintln!("Failed to open GitHub issue URL: {}", e);
                            println!("Please create an issue manually at: https://github.com/{ORG}/{REPO}/issues/new");
                        }
                    }
                }
                Ok(false) => {
                    println!("No GitHub issue created. Application will exit.");
                }
                Err(e) => {
                    eprintln!("Error asking for confirmation: {}", e);
                }
            }
        }).join();
    }));
}
