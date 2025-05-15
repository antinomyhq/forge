use anyhow::Result;
use inquire::Confirm;
use reqwest::Client;
use std::panic::{self, PanicHookInfo};
use std::{backtrace::Backtrace, fmt::Write as _};
use sysinfo::System;

use crate::{EventKind, Tracker};

#[derive(Debug, Clone)]
pub struct PanicReport {
    pub message: String,
    pub stack_trace: String,
    pub system_info: SystemInfo,
}

#[derive(Debug, Clone)]
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
        let backtrace = Backtrace::capture();
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
    repo_owner: String,
    repo_name: String,
    token: String,
}

impl GithubIssueCreator {
    pub fn new(token: String, repo_owner: String, repo_name: String) -> Self {
        Self { client: Client::new(), repo_owner, repo_name, token }
    }

    pub async fn create_issue(
        &self,
        title: String,
        body: String,
        labels: Vec<String>,
    ) -> Result<String> {
        #[derive(serde::Serialize)]
        struct IssueRequest<'a> {
            title: &'a str,
            body: &'a str,
            labels: &'a [String],
        }

        let request = IssueRequest { title: &title, body: &body, labels: &labels };

        let url = format!(
            "https://api.github.com/repos/{}/{}/issues",
            self.repo_owner, self.repo_name
        );

        let response = self
            .client
            .post(&url)
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

pub fn install_panic_hook() {
    panic::set_hook(Box::new(move |panic_info| {
        let report = PanicReport::from_panic_info(panic_info);
        let report_clone = report.clone();
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Send report to PostHog
        let _ = std::thread::spawn(move || {
            rt.block_on(async {
                // collect the event
                let _ = Tracker::default()
                    .dispatch(EventKind::Panic(report_clone.message.clone()))
                    .await;
            });
        })
        .join();

        // Ask user if they want to create a GitHub issue
        let report_formatted = report.to_markdown();
        // Print crash information
        eprintln!("\n\n{}\n\n", report.message);
        eprintln!("A panic occurred in the application!");

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            // Ask user if they want to create a GitHub issue
            match Confirm::new("Would you like to create a GitHub issue with the crash report?")
                .with_default(true)
                .prompt()
            {
                Ok(true) => {
                    todo!()
                }
                Ok(false) => {
                    println!("No GitHub issue created. Application will exit.");
                }
                Err(e) => {
                    eprintln!("Error asking for confirmation: {}", e);
                }
            }
        });
    }));
}
