use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_domain::ValidationRepository;
use forge_template::Element;
use tonic::transport::Channel;
use tracing::{debug, warn};

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
    /// Create a new repository from an existing gRPC channel
    ///
    /// # Arguments
    /// * `channel` - A shared gRPC channel to the workspace server
    pub fn new(channel: Channel) -> Self {
        let client = ForgeServiceClient::new(channel);
        Self { client }
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

        debug!(path = %path_str, "Starting syntax validation");

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
                proto_generated::validation_status::Status::Valid(_) => {
                    debug!(path = %path_str, "Syntax validation passed");
                    Ok(None)
                }
                proto_generated::validation_status::Status::Errors(error_list) => {
                    if error_list.errors.is_empty() {
                        return Ok(None);
                    }

                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("unknown");

                    let error_element = Element::new("warning")
                        .append(Element::new("message").text("Syntax validation failed"))
                        .append(
                            Element::new("file")
                                .attr("path", path.display().to_string())
                                .attr("extension", ext),
                        )
                        .append(Element::new("details").text(format!(
                            "The file was written successfully but contains {} syntax error(s)",
                            error_list.errors.len()
                        )))
                        .append(error_list.errors.iter().map(|error| {
                            warn!(
                                path = %path_str,
                                extension = ext,
                                error_count = error_list.errors.len(),
                                error_line = error.line,
                                error_column = error.column,
                                error_message = %error.message,
                                "Syntax validation failed"
                            );

                            Element::new("error")
                                .attr("line", error.line.to_string())
                                .attr("column", error.column.to_string())
                                .text(&error.message)
                        }))
                        .append(
                            Element::new("suggestion").text("Review and fix the syntax issues"),
                        );

                    Ok(Some(error_element.render()))
                }
            },
            _ => Ok(None), // No status or unsupported file type
        }
    }
}
