//! AWS credential provider for Bedrock.
//!
//! This module provides credential handling for AWS Bedrock using the
//! AWS SDK's default credential chain.

/// Creates an AWS config for Bedrock using the AWS SDK's default credential
/// chain.
///
/// The credential chain automatically looks for credentials in this order:
/// 1. Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY,
///    AWS_SESSION_TOKEN)
/// 2. AWS credentials file (~/.aws/credentials)
/// 3. AWS config file (~/.aws/config)
/// 4. IAM role (for EC2/ECS/Lambda)
/// 5. Container credentials (ECS)
///
/// # Arguments
///
/// * `region` - The AWS region (defaults to us-east-1 if None)
///
/// # Errors
///
/// Returns an error if the AWS config cannot be built.
pub async fn create_bedrock_config(
    region: Option<String>,
) -> anyhow::Result<aws_config::SdkConfig> {
    let region = region.unwrap_or_else(|| "us-east-1".to_string());

    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(region))
        .load()
        .await;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_bedrock_config_with_region() {
        let region = Some("us-west-2".to_string());
        let config = create_bedrock_config(region).await.unwrap();
        assert_eq!(config.region().unwrap().as_ref(), "us-west-2");
    }

    #[tokio::test]
    async fn test_create_bedrock_config_default_region() {
        let config = create_bedrock_config(None).await.unwrap();
        assert_eq!(config.region().unwrap().as_ref(), "us-east-1");
    }
}
