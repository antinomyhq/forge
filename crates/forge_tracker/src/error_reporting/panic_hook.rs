use std::panic;

use super::{github, output};
use crate::error_reporting::github::GithubIssueCreator;
use crate::error_reporting::report::PanicReport;
use crate::{EventKind, Tracker};

/// Installs a panic hook for handling application crashes
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
        output::important(format!("\n\n{}\n", report.message));
        let _ = std::thread::spawn(move || {

            // Ask user if they want to create a GitHub issue
            match inquire::Confirm::new("Would you like to create a GitHub issue with the crash report?")
                .with_default(true)
                .prompt()
            {
                Ok(true) => {
                    // Save the crash report to file first to ensure it's always saved
                    match github::save_crash_report_to_file(&report_formatted) {
                        Ok(_) => {}, // Success message is already handled in the function
                        Err(e) => {
                            output::error_details("Failed to save crash report", e);
                        }
                    }

                    // Generate a title for the issue
                    let title = "Crash Report".to_string();

                    // First try to get token from environment
                    let mut github_token = std::env::var("GITHUB_TOKEN").ok();

                    // If token exists in environment, make sure it's also saved to .env
                    if let Some(ref token) = github_token {
                        if !token.trim().is_empty() {
                            if let Err(e) = github::save_token_to_env(token) {
                                output::error_details("Failed to save token from environment to .env file", e);
                            }
                        }
                    } else {
                        // If no token in environment, ask for one (which will also save it)
                        output::info("No GitHub token found in environment.");
                        github_token = github::ask_for_github_token();
                    }

                    if let Some(token) = github_token {
                        if token.trim().is_empty() {
                            // Empty token provided, redirect to GitHub issues page
                            output::info("No token provided.");
                            if let Err(e) = github::create_github_issue_via_url(&title, &report_formatted) {
                                output::error_details("Failed to open GitHub issue URL", e);
                                output::instruction(format!("Please create an issue manually at: https://github.com/{}/{}/issues/new", github::ORG, github::REPO));
                            }
                        } else {
                            // Use the GitHub API with the provided token
                            output::action("Creating GitHub issue using API...");
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
                                    output::success(format!("GitHub issue created successfully: {issue_url}"));
                                    // Try to open the browser
                                    if let Err(e) = open::that(&issue_url) {
                                        output::error_details("Couldn't open browser automatically", e);
                                        output::instruction("Please visit the issue URL manually:");
                                        output::raw(&issue_url);
                                    }
                                }
                                Err(e) => {
                                    output::error_details("Failed to create GitHub issue via API", e);
                                    // Fallback to URL method
                                    if let Err(url_err) = github::create_github_issue_via_url(&title, &report_formatted) {
                                        output::error_details("Also failed to open GitHub issue URL", url_err);
                                        output::instruction(format!("Please create an issue manually at: https://github.com/{}/{}/issues/new", github::ORG, github::REPO));
                                    }
                                }
                            }
                        }
                    } else {
                        // No token was provided, redirect to GitHub issues page
                        if let Err(e) = github::create_github_issue_via_url(&title, &report_formatted) {
                            output::error_details("Failed to open GitHub issue URL", e);
                            output::instruction(format!("Please create an issue manually at: https://github.com/{}/{}/issues/new", github::ORG, github::REPO));
                        }
                    }
                }
                Ok(false) => {
                    output::info("No GitHub issue created. Application will exit.");
                }
                Err(e) => {
                    output::error_details("Error asking for confirmation", e);
                }
            }
        }).join();
    }));
}
