use std::path::PathBuf;

use anyhow::Context;

pub struct TempDir {
    temp_dir: tempfile::TempDir,
}

impl TempDir {
    const START_MARKER: &'static str = "___START___";
    const END_MARKER: &'static str = "___END___";
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = Self::temp_dir()?;
        Ok(Self {
            temp_dir: tempfile::Builder::new()
                .prefix(Self::START_MARKER)
                .suffix(Self::END_MARKER)
                .tempdir_in(temp_dir.clone())
                .with_context(|| {
                    format!("failed to create temp directory in: {}", temp_dir.display())
                })?,
        })
    }

    pub fn path(&self) -> std::path::PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    fn temp_dir() -> anyhow::Result<PathBuf> {
        Ok(std::env::temp_dir().canonicalize()?)
    }

    /// Use this function to remove the temporary directory path from the
    /// content
    pub fn normalize(input: &str) -> String {
        let pattern = format!(
            "{}/{}.*{}",
            Self::temp_dir().unwrap().display(),
            Self::START_MARKER,
            Self::END_MARKER
        );
        let pattern = regex::Regex::new(&pattern).unwrap();
        let content = pattern.replace_all(input, "[TEMP_DIR]");
        content.to_string()
    }
}
