//! Amazon Bedrock provider implementation using AWS SDK.
//!
//! This module provides integration with Amazon Bedrock's Converse API through
//! the official AWS Rust SDK. It supports chat completions, streaming, tool
//! calling, and regional model variants.
//!
//! # Setup
//!
//! To use Bedrock, you need:
//! 1. An AWS bearer token set in `AWS_BEARER_TOKEN_BEDROCK` environment
//!    variable
//! 2. An AWS region set in `AWS_REGION` (defaults to `us-east-1`)
//!
//! # Model IDs
//!
//! Bedrock supports various model families:
//! - **Anthropic Claude**: `anthropic.claude-3-5-sonnet-20241022-v2:0`
//! - **Amazon Nova**: `amazon.nova-pro-v1:0`, `amazon.nova-lite-v1:0`
//! - **Meta Llama**: `meta.llama3-1-70b-instruct-v1:0`
//!
//! Regional variants use prefixes:
//! - US regions: `us.anthropic.claude-3-5-sonnet-20241022-v2:0`
//! - EU regions: `eu.anthropic.claude-3-5-sonnet-20241022-v2:0`
//! - APAC regions: `apac.` or `au.` prefix
//!
//! # Features
//!
//! - ✅ Chat completions (non-streaming)
//! - ✅ Streaming responses
//! - ✅ Tool calling
//! - ✅ Regional model support
//! - ✅ Usage tracking
//! - ✅ Bearer token authentication

mod provider;
mod set_cache;

pub use provider::BedrockProvider;
pub use set_cache::SetCache;
