use crate::db::Database;

/// Extracts and validates the Bearer token from gRPC request metadata.
///
/// Returns the `user_id` associated with the token on success.
/// Returns `tonic::Status::unauthenticated` if the token is missing, malformed, or invalid.
pub async fn authenticate<T>(
    db: &Database,
    request: &tonic::Request<T>,
) -> Result<String, tonic::Status> {
    let header = request
        .metadata()
        .get("authorization")
        .ok_or_else(|| tonic::Status::unauthenticated("Missing authorization header"))?
        .to_str()
        .map_err(|_| tonic::Status::unauthenticated("Invalid authorization header encoding"))?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| tonic::Status::unauthenticated("Authorization header must use Bearer scheme"))?;

    let user_id = db
        .validate_api_key(token)
        .await
        .map_err(|e| tonic::Status::internal(format!("Auth lookup failed: {e}")))?
        .ok_or_else(|| tonic::Status::unauthenticated("Invalid API key"))?;

    Ok(user_id)
}
