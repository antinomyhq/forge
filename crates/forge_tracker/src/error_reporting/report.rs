use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::panic::PanicHookInfo;

use sysinfo::System;

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

        Self::new(format!("{message}{location}"), format!("{backtrace:#?}"))
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
