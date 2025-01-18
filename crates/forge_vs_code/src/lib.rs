mod vs_code;
#[cfg(target_os = "linux")]
mod linux;

pub use vs_code::*;

pub trait CodeInfo {
    fn hash_path(&self, folder_path: &str) -> anyhow::Result<String>;
    fn vs_code_path(&self) -> Option<String>;
}
