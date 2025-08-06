use strum_macros::{Display, EnumIter};

/// User response for permission confirmation requests
#[derive(Debug, Clone, PartialEq, Eq, Display, EnumIter)]
pub enum UserResponse {
    /// Accept the operation
    #[strum(to_string = "Accept")]
    Accept,
    /// Reject the operation
    #[strum(to_string = "Reject")]
    Reject,
    /// Accept the operation and remember this choice for similar operations
    #[strum(to_string = "Accept and Remember")]
    AcceptAndRemember,
}
