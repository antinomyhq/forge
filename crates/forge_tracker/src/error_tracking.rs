use crate::{EventKind, ErrorDetails, Tracker};
use std::backtrace::Backtrace;
use lazy_static::lazy_static;

lazy_static! {
    static ref TRACKER: Tracker = Tracker::default();
}


pub async fn track_error(
    error_type: impl Into<String>,
    message: impl Into<String>,
    context: impl Into<String>,
) -> crate::Result<()> {
    let details = ErrorDetails {
        error_type: error_type.into(),
        message: message.into(),
        context: context.into(),
        stack_trace: Some(Backtrace::force_capture().to_string()),
    };

    TRACKER.dispatch(EventKind::ErrorOccurred(details)).await
}

pub async fn track_error_without_trace(
    error_type: impl Into<String>,
    message: impl Into<String>,
    context: impl Into<String>,
) -> crate::Result<()> {
    let details = ErrorDetails {
        error_type: error_type.into(),
        message: message.into(),
        context: context.into(),
        stack_trace: None,
    };

    TRACKER.dispatch(EventKind::ErrorOccurred(details)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_track_error() {
        let result = track_error(
            "TestError",
            "Test error message",
            "test_track_error function"
        ).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_track_error_without_trace() {
        let result = track_error_without_trace(
            "TestError",
            "Test error message",
            "test_track_error_without_trace function"
        ).await;
        assert!(result.is_ok());
    }
} 