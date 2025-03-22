use forge_tracker::Tracker;
use lazy_static::lazy_static;

lazy_static! {
    static ref TRACKER: Tracker = Tracker::default();
}

#[tokio::test]
async fn test_dispatch_error() {
    let error_type = "TestError".to_string();
    let error_message = "This is a test error".to_string();
    let context = "test_dispatch_error".to_string();
    let stack_trace = Some("stack trace details".to_string());

    if let Err(e) = TRACKER
        .dispatch_error(error_type, error_message, context, stack_trace)
        .await
    {
        panic!("Tracker dispatch error: {:?}", e);
    }
}

#[tokio::test]
async fn test_dispatch_error_without_stack_trace() {
    let error_type = "ValidationError".to_string();
    let error_message = "Invalid input parameters".to_string();
    let context = "form_validation".to_string();
    let stack_trace = None;

    let result = TRACKER
        .dispatch_error(error_type, error_message, context, stack_trace)
        .await;
    
    assert!(result.is_ok(), "Error dispatch failed: {:?}", result.err());
}

#[tokio::test]
async fn test_dispatch_error_with_empty_strings() {
    let error_type = "".to_string();
    let error_message = "".to_string();
    let context = "".to_string();
    let stack_trace = Some("".to_string());

    let result = TRACKER
        .dispatch_error(error_type, error_message, context, stack_trace)
        .await;
    
    assert!(result.is_ok(), "Error dispatch with empty strings failed: {:?}", result.err());
}

#[tokio::test]
async fn test_dispatch_multiple_errors() {
    // First error
    let result1 = TRACKER
        .dispatch_error(
            "NetworkError".to_string(),
            "Connection timeout".to_string(),
            "api_request".to_string(),
            Some("Network stack trace".to_string()),
        )
        .await;
    
    // Second error
    let result2 = TRACKER
        .dispatch_error(
            "DatabaseError".to_string(),
            "Failed to execute query".to_string(),
            "data_retrieval".to_string(),
            Some("Database stack trace".to_string()),
        )
        .await;
    
    assert!(result1.is_ok() && result2.is_ok(), "Multiple error dispatch failed");
}

#[tokio::test]
async fn test_dispatch_error_with_long_message() {
    let error_type = "SystemError".to_string();
    let error_message = "X".repeat(10000); // Very long error message
    let context = "system_operation".to_string();
    let stack_trace = Some("Detailed system stack trace".to_string());

    let result = TRACKER
        .dispatch_error(error_type, error_message, context, stack_trace)
        .await;
    
    assert!(result.is_ok(), "Error dispatch with long message failed: {:?}", result.err());
}