use forge_display::TitleFormat;
use serde_json::Value;

fn parse_error_message(err_msg: String) -> Result<String, anyhow::Error> {
    let json_start = err_msg
        .find("Reason: ")
        .map(|i| i + "Reason: ".len())
        .unwrap_or(0);
    let json_str = &err_msg[json_start..];

    let outer: Value = serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse the response message: {}", e))?;
    let message_str = outer["message"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'message' field"))?;
    let error: Value = serde_json::from_str(message_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse inner error message: {}", e))?;
    let details = error["error"]["details"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'details' field"))?;

    Ok(details.to_owned())
}

pub fn usage_limits(err: &anyhow::Error) -> Result<Option<TitleFormat>, anyhow::Error> {
    if let Some(err) = err
        .chain()
        .map(|e| e.to_string())
        .find(|e| e.contains("USAGE_LIMIT_EXCEEDED"))
    {
        let parsed_err_msg = parse_error_message(err)?;
        Ok(Some(TitleFormat::upgrade(parsed_err_msg)))
    } else {
        Ok(None)
    }
}

pub fn unsupported_model(err: &anyhow::Error) -> Result<Option<TitleFormat>, anyhow::Error> {
    if let Some(err) = err
        .chain()
        .map(|e| e.to_string())
        .find(|e| e.contains("UNSUPPORTED_MODEL"))
    {
        let parsed_err_msg = parse_error_message(err)?;
        Ok(Some(TitleFormat::upgrade(parsed_err_msg)))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use forge_display::TitleFormat;
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_usage_limit_error_fixture() -> String {
        r#"Some error occurred. Reason: {"message": "{\"error\": {\"details\": \"You have exceeded your usage limit. Please upgrade your plan.\"}}"}"#.to_string()
    }

    fn create_unsupported_model_error_fixture() -> String {
        r#"Model error. Reason: {"message": "{\"error\": {\"details\": \"The requested model is not supported in your region.\"}}"}"#.to_string()
    }

    fn create_invalid_json_error_fixture() -> String {
        r#"Invalid error. Reason: invalid json"#.to_string()
    }

    fn create_missing_fields_error_fixture() -> String {
        r#"Missing fields. Reason: {"message": "{\"error\": {\"missing\": \"details\"}}"}"#
            .to_string()
    }

    #[test]
    fn test_parse_error_message_success() {
        let fixture = create_usage_limit_error_fixture();
        let actual = parse_error_message(fixture).unwrap();
        let expected = "You have exceeded your usage limit. Please upgrade your plan.".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_error_message_without_reason_prefix() {
        let fixture =
            r#"{"message": "{\"error\": {\"details\": \"Direct error message\"}}"}"#.to_string();
        let actual = parse_error_message(fixture).unwrap();
        let expected = "Direct error message".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_parse_error_message_invalid_json() {
        let fixture = create_invalid_json_error_fixture();
        let actual = parse_error_message(fixture);
        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Failed to parse the response message")
        );
    }

    #[test]
    fn test_parse_error_message_missing_message_field() {
        let fixture = r#"{"error": "no message field"}"#.to_string();
        let actual = parse_error_message(fixture);
        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Missing 'message' field")
        );
    }

    #[test]
    fn test_parse_error_message_missing_details_field() {
        let fixture = create_missing_fields_error_fixture();
        let actual = parse_error_message(fixture);
        assert!(actual.is_err());
        assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("Missing 'details' field")
        );
    }

    #[test]
    fn test_usage_limits_with_non_matching_error() {
        let fixture = anyhow!("Some other error");
        let actual = usage_limits(&fixture).unwrap();
        let expected = None;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_unsupported_model_with_non_matching_error() {
        let fixture = anyhow!("Some other error");
        let actual = unsupported_model(&fixture).unwrap();
        let expected = None;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_both_functions_with_empty_error_chain() {
        let fixture = anyhow!("Simple error");

        let usage_actual = usage_limits(&fixture).unwrap();
        let usage_expected = None;
        assert_eq!(usage_actual, usage_expected);

        let model_actual = unsupported_model(&fixture).unwrap();
        let model_expected = None;
        assert_eq!(model_actual, model_expected);
    }

    #[test]
    fn test_usage_limits_error_propagation() {
        let error_msg = format!(
            "USAGE_LIMIT_EXCEEDED: {}",
            create_invalid_json_error_fixture()
        );
        let fixture = anyhow!(error_msg);
        let actual = usage_limits(&fixture);
        assert!(actual.is_err());
    }

    #[test]
    fn test_unsupported_model_error_propagation() {
        let error_msg = format!("UNSUPPORTED_MODEL: {}", create_invalid_json_error_fixture());
        let fixture = anyhow!(error_msg);
        let actual = unsupported_model(&fixture);
        assert!(actual.is_err());
    }

    // Tests for actual usage - these need to be written differently since anyhow!
    // creates a different error structure
    #[test]
    fn test_usage_limits_with_matching_error_in_chain() {
        // Create an error that contains the target string in the error chain
        let error_msg = format!(
            "USAGE_LIMIT_EXCEEDED: {}",
            create_usage_limit_error_fixture()
        );
        let fixture = anyhow!(error_msg);
        let actual = usage_limits(&fixture).unwrap();
        let expected = Some(TitleFormat::upgrade(
            "You have exceeded your usage limit. Please upgrade your plan.".to_string(),
        ));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_unsupported_model_with_matching_error_in_chain() {
        // Create an error that contains the target string in the error chain
        let error_msg = format!(
            "UNSUPPORTED_MODEL: {}",
            create_unsupported_model_error_fixture()
        );
        let fixture = anyhow!(error_msg);
        let actual = unsupported_model(&fixture).unwrap();
        let expected = Some(TitleFormat::upgrade(
            "The requested model is not supported in your region.".to_string(),
        ));
        assert_eq!(actual, expected);
    }
}
