use colored::Colorize;
use forge_domain::Error as DomainError;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    message: String,
    reason: Option<String>,
    solution: Option<String>,
}

impl Error {
    pub fn print(&self) {
        println!("{}", "🛑 Something went wrong".red());
        if let Some(reason) = &self.reason {
            println!("🔍 Reason: {}", reason.red());
        }
        if let Some(solution) = &self.reason {
            println!("💡 Solution: {}", solution.red());
        }
    }
}

impl From<&DomainError> for Error {
    fn from(value: &DomainError) -> Self {
        match &value {
            DomainError::AuthenticationError(_) => Self {
                message: value.to_string(),
                reason: Some("The provided credentials are invalid.".to_string()),
                solution: None,
            },
            _ => Self { message: value.to_string(), reason: None, solution: None },
        }
    }
}
