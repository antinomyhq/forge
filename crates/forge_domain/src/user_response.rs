/// User response for permission confirmation requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserResponse {
    /// Accept the operation
    Accept,
    /// Reject the operation
    Reject,
    /// Accept the operation and remember this choice for similar operations
    AcceptAndRemember,
}
