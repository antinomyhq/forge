use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::GrpcInfra;
use forge_domain::ValidationRepository;
use forge_template::Element;
use tracing::{debug, warn};

#[cfg(feature = "tree_sitter_validation")]
use crate::tree_sitter_impl;

// Include the generated proto code at module level
#[allow(dead_code)]
mod proto_generated {
    tonic::include_proto!("forge.v1");
}

use forge_service_client::ForgeServiceClient;
use proto_generated::*;

/// gRPC implementation of ValidationRepository
#[derive(Clone)]
pub struct ForgeValidationRepository<I> {
    infra: Arc<I>,
}

impl<I> ForgeValidationRepository<I> {
    /// Create a new repository with the given infrastructure
    ///
    /// # Arguments
    /// * `infra` - Infrastructure that provides gRPC connection
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }
}
/// Factory for creating validation repositories based on environment
/// configuration Enum wrapper for different validation repository
/// implementations
#[derive(Clone)]
pub enum ValidationRepositoryWrapper<F> {
    #[cfg(feature = "tree_sitter_validation")]
    TreeSitter(TreeSitterValidationRepository),
    Remote(ForgeValidationRepository<F>),
}

#[async_trait]
impl<F: GrpcInfra> ValidationRepository for ValidationRepositoryWrapper<F> {
    async fn validate_file(
        &self,
        path: impl AsRef<Path> + Send,
        content: &str,
    ) -> Result<Option<String>> {
        match self {
            #[cfg(feature = "tree_sitter_validation")]
            ValidationRepositoryWrapper::TreeSitter(repo) => {
                repo.validate_file(path, content).await
            }
            ValidationRepositoryWrapper::Remote(repo) => repo.validate_file(path, content).await,
        }
    }
}
pub struct ValidationRepositoryFactory;

impl ValidationRepositoryFactory {
    /// Create a validation repository based on environment and feature flags
    pub fn create<F: GrpcInfra>(infra: Arc<F>) -> ValidationRepositoryWrapper<F> {
        let use_tree_sitter = match std::env::var("FORGE_USE_TREE_SITTER").as_deref() {
            Ok("1") | Ok("true") => true,
            _ => false,
        };

        if use_tree_sitter {
            #[cfg(feature = "tree_sitter_validation")]
            {
                tracing::info!("Using tree-sitter validation");
                ValidationRepositoryWrapper::TreeSitter(TreeSitterValidationRepository::new())
            }
            #[cfg(not(feature = "tree_sitter_validation"))]
            {
                tracing::warn!("Tree-sitter validation requested but not compiled with tree_sitter_validation feature, falling back to remote validation");
                ValidationRepositoryWrapper::Remote(ForgeValidationRepository::new(infra))
            }
        } else {
            tracing::info!("Using remote validation");
            ValidationRepositoryWrapper::Remote(ForgeValidationRepository::new(infra))
        }
    }
}

#[async_trait]
impl<I: GrpcInfra> ValidationRepository for ForgeValidationRepository<I> {
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
        let channel = self.infra.channel();
        let mut client = ForgeServiceClient::new(channel);
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
                                .cdata(&error.message)
                        }))
                        .append(
                            Element::new("suggestion").text("Review and fix the syntax issues"),
                        );

                    Ok(Some(error_element.render()))
                }
                proto_generated::validation_status::Status::UnsupportedLanguage(_) => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("unknown");
                    debug!(
                        path = %path_str,
                        extension = ext,
                        "Syntax validation skipped: unsupported language"
                    );
                    Ok(None)
                }
            },
            _ => Ok(None),
        }
    }
}

/// Tree-sitter implementation of ValidationRepository
#[cfg(feature = "tree_sitter_validation")]
#[derive(Clone)]
pub struct TreeSitterValidationRepository;

#[cfg(feature = "tree_sitter_validation")]
impl TreeSitterValidationRepository {
    /// Create a new tree-sitter validation repository
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "tree_sitter_validation")]
#[async_trait]
impl ValidationRepository for TreeSitterValidationRepository {
    async fn validate_file(
        &self,
        path: impl AsRef<Path> + Send,
        content: &str,
    ) -> Result<Option<String>> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();

        debug!(path = %path_str, "Starting tree-sitter syntax validation");

        // Determine language from file extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        let result = match ext {
            "rs" => self::tree_sitter_impl::validate_rust(content),
            "py" => self::tree_sitter_impl::validate_python(content),
            "js" | "mjs" | "cjs" => self::tree_sitter_impl::validate_javascript(content),
            "ts" | "tsx" => self::tree_sitter_impl::validate_typescript(content),
            "c" | "h" => self::tree_sitter_impl::validate_c(content),
            "cpp" | "cxx" | "hpp" | "hxx" => self::tree_sitter_impl::validate_cpp(content),
            "cs" => self::tree_sitter_impl::validate_c_sharp(content),
            "java" => self::tree_sitter_impl::validate_java(content),
            "go" => self::tree_sitter_impl::validate_go(content),
            "php" => self::tree_sitter_impl::validate_php(content),
            "rb" => self::tree_sitter_impl::validate_ruby(content),
            "swift" => self::tree_sitter_impl::validate_swift(content),
            "kt" | "kts" => self::tree_sitter_impl::validate_kotlin(content),
            "dart" => self::tree_sitter_impl::validate_dart(content),
            "html" | "htm" => self::tree_sitter_impl::validate_html(content),
            "css" => self::tree_sitter_impl::validate_css(content),
            "json" => self::tree_sitter_impl::validate_json(content),
            "yaml" | "yml" => self::tree_sitter_impl::validate_yaml(content),
            "toml" => self::tree_sitter_impl::validate_toml(content),
            "sh" | "bash" => self::tree_sitter_impl::validate_bash(content),
            "ps1" => self::tree_sitter_impl::validate_powershell(content),
            "sql" => self::tree_sitter_impl::validate_sql(content),
            "md" | "markdown" => self::tree_sitter_impl::validate_markdown(content),
            _ => {
                debug!(path = %path_str, extension = ext, "Unsupported file type for tree-sitter validation");
                return Ok(None);
            }
        };

        match result {
            Ok(errors) if errors.is_empty() => {
                debug!(path = %path_str, "Tree-sitter validation passed");
                Ok(None)
            }
            Ok(errors) => {
                warn!(
                    path = %path_str,
                    extension = ext,
                    error_count = errors.len(),
                    "Tree-sitter validation failed"
                );

                let error_element = Element::new("warning")
                    .append(Element::new("message").text("Syntax validation failed"))
                    .append(
                        Element::new("file")
                            .attr("path", path.display().to_string())
                            .attr("extension", ext),
                    )
                    .append(Element::new("details").text(format!(
                        "The file was written successfully but contains {} syntax error(s)",
                        errors.len()
                    )))
                    .append(errors.iter().map(|error| {
                        Element::new("error")
                            .attr("line", error.line.to_string())
                            .attr("column", error.column.to_string())
                            .cdata(&error.message)
                    }))
                    .append(Element::new("suggestion").text("Review and fix syntax issues"));

                Ok(Some(error_element.render()))
            }
            Err(e) => {
                warn!(
                    path = %path_str,
                    extension = ext,
                    error = %e,
                    "Tree-sitter validation error"
                );
                // Don't block file operations due to validation errors
                Ok(None)
            }
        }
    }
}
