#[derive(Debug, Clone)]
pub struct Mode(String);

impl Default for Mode {
    fn default() -> Self {
        Self("ACT".to_string())
    }
}

impl AsRef<str> for Mode {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Mode {
    pub fn new<T: AsRef<str>>(value: T) -> Self {
        Self(value.as_ref().to_string())
    }
}
