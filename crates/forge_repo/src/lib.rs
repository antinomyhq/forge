use tonic::transport::Channel;

mod agent;
mod agent_definition;
mod context_engine;
mod conversation;
mod database;
mod forge_repo;
mod fs_snap;
mod fuzzy_search;
mod provider;
mod skill;
mod validation;

mod proto_generated {
    tonic::include_proto!("forge.v1");
}

const MAX_GRPC_MESSAGE_SIZE_BYTES: usize = 64 * 1024 * 1024;

/// Creates a ForgeService gRPC client with elevated message-size limits.
///
/// # Arguments
///
/// * `channel` - Shared gRPC transport channel for ForgeService RPCs
pub(crate) fn new_forge_service_client(
    channel: Channel,
) -> proto_generated::forge_service_client::ForgeServiceClient<Channel> {
    proto_generated::forge_service_client::ForgeServiceClient::new(channel)
        .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE_BYTES)
        .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE_BYTES)
}

// Only expose forge_repo container
pub use forge_repo::*;
