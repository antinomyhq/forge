use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_domain::ValidationRepository;
use tonic::transport::Channel;

// Include the generated proto code at module level
#[allow(dead_code)]
mod proto_generated {
    tonic::include_proto!("forge.v1");
}

use forge_service_client::ForgeServiceClient;
use proto_generated::*;

/// gRPC implementation of ValidationRepository
pub struct ForgeValidationRepository {
    client: ForgeServiceClient<Channel>,
}

impl ForgeValidationRepository {
    /// Create a new gRPC client with lazy connection
    ///
    /// # Arguments
    /// * `server_url` - The URL of the validation server
    ///
    /// # Errors
    /// Returns an error if the channel cannot be created
    pub fn new(server_url: &url::Url) -> Result<Self> {
        let mut channel = Channel::from_shared(server_url.to_string())?.concurrency_limit(256);

        // Enable TLS for https URLs using system certificate store
        if server_url.scheme().contains("https") {
            channel =
                channel.tls_config(tonic::transport::ClientTlsConfig::new().with_native_roots())?;
        }

        let channel = channel.connect_lazy();
        let client = ForgeServiceClient::new(channel);

        Ok(Self { client })
    }
}

#[async_trait]
impl ValidationRepository for ForgeValidationRepository {
    async fn validate_file(
        &self,
        path: impl AsRef<Path> + Send,
        content: &str,
    ) -> Result<Option<String>> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();

        // Create validation request for single file
        let proto_file = File { path: path_str.clone(), content: content.to_string() };
        let request = tonic::Request::new(ValidateFilesRequest { files: vec![proto_file] });

        // Call gRPC API
        let mut client = self.client.clone();
        let response = client
            .validate_files(request)
            .await
            .context("Failed to call ValidateFiles gRPC")?
            .into_inner();

        // Extract validation result for our file
        let result = response
            .results
            .into_iter()
            .find(|r| r.file_path == path_str)
            .context("Validation response missing file result")?;

        // Convert proto status to error message
        match result.status {
            Some(proto_generated::ValidationStatus { status: Some(status) }) => match status {
                proto_generated::validation_status::Status::Valid(_) => Ok(None),
                proto_generated::validation_status::Status::Errors(error_list) => {
                    if error_list.errors.is_empty() {
                        return Ok(None);
                    }

                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("unknown");
                    let first_error = &error_list.errors[0];

                    let error_msg = format!(
                        "Syntax validation failed for {} ({}): The file was written successfully but contains {} syntax error(s). First error at line {}, column {}: {}. Suggestion: Review and fix the syntax issues.",
                        path.display(),
                        ext,
                        error_list.errors.len(),
                        first_error.line,
                        first_error.column,
                        first_error.message
                    );
                    Ok(Some(error_msg))
                }
            },
            _ => Ok(None), // No status or unsupported file type
        }
    }
}
