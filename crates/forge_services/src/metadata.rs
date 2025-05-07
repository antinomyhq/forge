use std::fmt::Display;

/// Holds Metadata about truncating, file paths, chars ranges.
#[derive(Default)]
pub struct Metadata(Vec<(&'static str, String)>);

impl Display for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "---")?;
        for (k, v) in self.0.iter() {
            writeln!(f, "{k}: {v}")?;
        }
        writeln!(f, "---")
    }
}

impl Metadata {
    /// Add a key-value pair to the metadata
    pub fn add<S: ToString>(mut self, key: &'static str, value: S) -> Self {
        self.0.push((key, value.to_string()));
        self
    }

    /// Add a key-value pair to the metadata only if the value is Some
    ///
    /// This is a convenience method for conditionally adding metadata
    /// without needing to use if-else blocks
    pub fn add_optional<S: ToString>(self, key: &'static str, value: Option<S>) -> Self {
        match value {
            Some(v) => self.add(key, v),
            None => self,
        }
    }
}
