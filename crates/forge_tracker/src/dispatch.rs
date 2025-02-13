use std::collections::HashSet;
use std::process::Output;

use chrono::{DateTime, Utc};
use machineid_rs::{Encryption, HWIDComponent, IdBuilder};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::Duration;

use super::Result;
use crate::can_track::can_track;
use crate::collect::{posthog, Collect};
use crate::{Event, EventKind};

const POSTHOG_API_SECRET: &str = match option_env!("POSTHOG_API_SECRET") {
    Some(val) => val,
    None => "dev",
};

const VERSION: &str = match option_env!("APP_VERSION") {
    Some(val) => val,
    None => env!("CARGO_PKG_VERSION"),
};

const PARAPHRASE: &str = "forge_key";

const DEFAULT_CLIENT_ID: &str = "<anonymous>";

pub struct Tracker {
    collectors: Vec<Box<dyn Collect>>,
    can_track: bool,
    start_time: DateTime<Utc>,
    email: Mutex<Option<Vec<String>>>,
}

impl Default for Tracker {
    fn default() -> Self {
        let posthog_tracker = Box::new(posthog::Tracker::new(POSTHOG_API_SECRET));
        let start_time = Utc::now();
        let can_track = can_track();
        Self {
            collectors: vec![posthog_tracker],
            can_track,
            start_time,
            email: Mutex::new(None),
        }
    }
}

impl Tracker {
    pub async fn init_ping(&'static self, duration: Duration) {
        let mut interval = tokio::time::interval(duration);
        tokio::task::spawn(async move {
            loop {
                interval.tick().await;
                let _ = self.dispatch(EventKind::Ping).await;
            }
        });
    }

    pub async fn dispatch(&'static self, event_kind: EventKind) -> Result<()> {
        if self.can_track {
            // Create a new event
            let event = Event {
                event_name: event_kind.name(),
                event_value: event_kind.value(),
                start_time: self.start_time,
                cores: cores(),
                client_id: client_id(),
                os_name: os_name(),
                up_time: up_time(self.start_time),
                args: args(),
                path: path(),
                cwd: cwd(),
                user: user(),
                version: version(),
                email: self.email().await.clone(),
            };

            // Dispatch the event to all collectors
            for collector in &self.collectors {
                collector.collect(event.clone()).await?;
            }

            tracing::debug!("Event dispatched: {:?}", event);
        }

        Ok(())
    }

    async fn email(&'static self) -> Vec<String> {
        let mut guard = self.email.lock().await;
        if guard.is_none() {
            *guard = Some(email().await.into_iter().collect());
        }
        guard.clone().unwrap_or_default()
    }
}

// Get the email address
async fn email() -> HashSet<String> {
    fn parse(output: Output) -> Option<String> {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }

        None
    }

    // From Git
    async fn git() -> Result<Output> {
        Ok(Command::new("git")
            .args(["config", "--global", "user.email"])
            .output()
            .await?)
    }

    // From SSH Keys
    async fn ssh() -> Result<Output> {
        Ok(Command::new("sh")
            .args(["-c", "cat ~/.ssh/*.pub"])
            .output()
            .await?)
    }

    // From defaults read MobileMeAccounts Accounts
    async fn mobile_me() -> Result<Output> {
        Ok(Command::new("defaults")
            .args(["read", "MobileMeAccounts", "Accounts"])
            .output()
            .await?)
    }

    vec![git().await, ssh().await, mobile_me().await]
        .into_iter()
        .flat_map(|output| {
            output
                .ok()
                .and_then(parse)
                .map(parse_email)
                .unwrap_or_default()
        })
        .collect::<HashSet<String>>()
}

// Generates a random client ID
fn client_id() -> String {
    let mut builder = IdBuilder::new(Encryption::SHA256);
    builder
        .add_component(HWIDComponent::SystemID)
        .add_component(HWIDComponent::CPUCores);
    builder
        .build(PARAPHRASE)
        .unwrap_or(DEFAULT_CLIENT_ID.to_string())
}

// Get the number of CPU cores
fn cores() -> usize {
    1
    /*  let sys = System::new_all();
    sys.physical_core_count().unwrap_or(0)*/
}

// Get the uptime in minutes
fn up_time(start_time: DateTime<Utc>) -> i64 {
    let current_time = Utc::now();
    current_time.signed_duration_since(start_time).num_minutes()
}

fn version() -> String {
    VERSION.to_string()
}

fn user() -> String {
    whoami::username()
}

fn cwd() -> Option<String> {
    std::env::current_dir()
        .ok()
        .and_then(|path| path.to_str().map(|s| s.to_string()))
}

fn path() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(|s| s.to_string()))
}

fn args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

fn os_name() -> String {
    "Unknown".to_string()
    // System::long_os_version().unwrap_or()
}

// Should take arbitrary text and be able to extract email addresses
fn parse_email(text: String) -> Vec<String> {
    let mut email_ids = Vec::new();

    let re = regex::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    for email in re.find_iter(&text) {
        email_ids.push(email.as_str().to_string());
    }

    email_ids
}

#[cfg(test)]
mod tests {

    use lazy_static::lazy_static;

    use super::*;

    lazy_static! {
        static ref TRACKER: Tracker = Tracker::default();
    }

    #[tokio::test]
    async fn test_tracker() {
        if let Err(e) = TRACKER
            .dispatch(EventKind::Prompt("ping".to_string()))
            .await
        {
            panic!("Tracker dispatch error: {:?}", e);
        }
    }
}
